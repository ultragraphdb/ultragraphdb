/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A network between the real frontend and the real system group that fails as a scenario says: the frontend dials the group
//! at `[::1]` and the link carries each connection to the group's `127.0.0.1` at the same port, which the config service's System
//! address check takes, since a host may differ. Frames pass whole and in order, since a frame's `sequence` must be one above
//! the last its sender sent (`PROTOCOL.md` §2.5, step 13). The link closes every connection it carries and refuses new ones
//! while it is severed, and holds every frame the group sends while it is stalled, closes and answers alike, so an endpoint
//! core decides from a view that falls behind the closes while its asks wait. It runs on threads of the test's own, in clear
//! text over TCP, since it reads each frame's header.

use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use ultragraphdb_protocol_wire::{Command, HEADER_SIZE};
use ultragraphdb_tool_bdd::{TestError, TestResult};

/// What the link holds between its threads.
#[derive(Debug, Default)]
struct LinkState {
    /// Whether the link refuses connections and has closed those it carried.
    severed: bool,
    /// Whether the frames the group sends wait in the link.
    stalled: bool,
    /// Both sockets of every connection carried, to close at a sever.
    open: Vec<TcpStream>,
    /// The connections carried.
    connections: u32,
    /// The closes held so far while stalled.
    held: u32,
}

/// A link from `[::1]` to the system group at `127.0.0.1`, at the group's port.
#[derive(Debug)]
pub(crate) struct GroupLink {
    address: SocketAddr,
    state: Arc<Mutex<LinkState>>,
    stopped: Arc<AtomicBool>,
}

impl GroupLink {
    /// A link to the system group at `system`, a `tcp://127.0.0.1:port` address, listening at `[::1]` and that port. The group
    /// need not listen yet: a connection the link cannot carry on closes, and the frontend dials again.
    #[expect(
        clippy::disallowed_methods,
        reason = "the link carries bytes beside the scenarios, on threads of the test's own"
    )]
    pub(crate) fn to(system: &str) -> TestResult<Self> {
        let target: SocketAddr = system
            .strip_prefix("tcp://")
            .ok_or_else(|| TestError::new(format!("a TCP address, not {system}")))?
            .parse()?;
        let listener = TcpListener::bind(SocketAddr::new("::1".parse()?, target.port()))?;
        let address = listener.local_addr()?;
        let state = Arc::new(Mutex::new(LinkState::default()));
        let stopped = Arc::new(AtomicBool::new(false));
        let (held, ending) = (Arc::clone(&state), Arc::clone(&stopped));
        drop(std::thread::spawn(move || {
            for accepted in listener.incoming().flatten() {
                if ending.load(Ordering::Relaxed) {
                    break;
                }
                if locked(&held).severed {
                    // Refused: the frontend's dial finds its connection closed.
                    drop(accepted.shutdown(Shutdown::Both));
                    continue;
                }
                // A connection the group refuses is what the frontend then sees: its socket closes.
                drop(carry(accepted, target, &held));
            }
        }));
        Ok(Self {
            address,
            state,
            stopped,
        })
    }

    /// Where the frontend dials the group: `cluster.system_address`.
    pub(crate) fn address(&self) -> String {
        format!("tcp://{}", self.address)
    }

    /// Closes every connection carried, and refuses new ones until [`GroupLink::restore`].
    pub(crate) fn sever(&self) {
        let open = {
            let mut state = locked(&self.state);
            state.severed = true;
            std::mem::take(&mut state.open)
        };
        for socket in open {
            drop(socket.shutdown(Shutdown::Both));
        }
    }

    /// Carries new connections again.
    pub(crate) fn restore(&self) {
        locked(&self.state).severed = false;
    }

    /// Holds every frame the group sends from now on, on every connection.
    pub(crate) fn stall(&self) {
        locked(&self.state).stalled = true;
    }

    /// Passes the frames held, in their order, each connection's with the next frame the group sends on it, a close at least
    /// every `epoch_ms_max`, and every later one.
    pub(crate) fn resume(&self) {
        locked(&self.state).stalled = false;
    }

    /// The connections carried so far.
    pub(crate) fn connections(&self) -> u32 {
        locked(&self.state).connections
    }

    /// The closes held so far while stalled.
    pub(crate) fn held(&self) -> u32 {
        locked(&self.state).held
    }
}

impl Drop for GroupLink {
    /// Stops listening, so a later link binds the address, and closes what it carries.
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        // The accepting thread takes this connection and ends.
        drop(TcpStream::connect(self.address));
        self.sever();
    }
}

fn locked(state: &Mutex<LinkState>) -> std::sync::MutexGuard<'_, LinkState> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Starts carrying the frontend's connection `frontend` to the group at `target`, on a thread each way.
#[expect(
    clippy::disallowed_methods,
    reason = "the link carries bytes beside the scenarios, on threads of the test's own"
)]
fn carry(
    frontend: TcpStream,
    target: SocketAddr,
    state: &Arc<Mutex<LinkState>>,
) -> std::io::Result<()> {
    let group = TcpStream::connect(target)?;
    frontend.set_nodelay(true)?;
    group.set_nodelay(true)?;
    {
        let mut held = locked(state);
        held.connections += 1;
        held.open.push(frontend.try_clone()?);
        held.open.push(group.try_clone()?);
    }
    let (up_from, up_to) = (frontend.try_clone()?, group.try_clone()?);
    drop(std::thread::spawn(move || {
        let (mut from, mut to) = (up_from, up_to);
        drop(std::io::copy(&mut from, &mut to));
        drop(from.shutdown(Shutdown::Both));
        drop(to.shutdown(Shutdown::Both));
    }));
    let down_state = Arc::clone(state);
    drop(std::thread::spawn(move || {
        down(group, frontend, &down_state);
    }));
    Ok(())
}

/// One frame from `from`, whole: `None` at the close.
fn next_frame(from: &mut TcpStream) -> Option<Vec<u8>> {
    let mut frame = vec![0; HEADER_SIZE];
    from.read_exact(&mut frame).ok()?;
    let size = frame.get(40..44)?;
    let size = u32::from_le_bytes(size.try_into().ok()?) as usize;
    frame.resize(size.max(HEADER_SIZE), 0);
    from.read_exact(frame.get_mut(HEADER_SIZE..)?).ok()?;
    Some(frame)
}

/// Whether `frame` is a close, by its header's command.
fn is_close(frame: &[u8]) -> bool {
    frame
        .get(54..56)
        .and_then(|number| number.try_into().ok())
        .map(u16::from_le_bytes)
        == Some(Command::EpochClose.number())
}

/// Carries the group's frames to the frontend until either closes, each waiting in `held` while the link is stalled.
fn down(mut group: TcpStream, mut frontend: TcpStream, state: &Mutex<LinkState>) {
    let mut held: Vec<Vec<u8>> = Vec::new();
    while let Some(frame) = next_frame(&mut group) {
        let stalled = {
            let mut shared = locked(state);
            if shared.stalled && is_close(&frame) {
                shared.held += 1;
            }
            shared.stalled
        };
        held.push(frame);
        if stalled {
            continue;
        }
        let sent = held
            .drain(..)
            .try_for_each(|frame| frontend.write_all(&frame));
        if sent.is_err() {
            break;
        }
    }
    drop(group.shutdown(Shutdown::Both));
    drop(frontend.shutdown(Shutdown::Both));
}
