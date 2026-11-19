/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A network between a real client and a real frontend that loses what a scenario says: frames pass whole, and a frame of the
//! command armed is lost with its connection, damaged on its way, or held until the scenario lets it go. Every process either
//! side of it is real and does as it would. It runs on threads of the test's own, in clear text over TCP, since it reads each
//! frame's header.

use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex, PoisonError};

use futures::channel::oneshot;
use ultragraphdb_protocol_wire::{Command, HEADER_SIZE, HeaderView};
use ultragraphdb_tool_bdd::{TestError, TestResult};

use crate::shells::ClientAt;

/// What the link does to the next frame of a command it was armed for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fault {
    /// The next request of this command never reaches the frontend, and the connection closes.
    LoseRequest(Command),
    /// The next reply of this command never reaches the client, and the connection closes.
    LoseReply(Command),
    /// The next reply of this command reaches the client with a bit of its byte `.1` flipped.
    FlipReply(Command, usize),
    /// The request of this command numbered `.1` from the arming, from 1, waits in the link, and every frame behind it on its
    /// way, until the link is released.
    HoldRequest(Command, usize),
}

/// What the link holds between its threads.
#[derive(Debug, Default)]
struct LinkState {
    armed: Option<Fault>,
    /// Each frame the link lost, whole, in the order it lost them.
    lost: Vec<Vec<u8>>,
    /// The connections it has carried.
    connections: u32,
    /// The stream of each request of the held command since the hold was armed, in the order they reached the link.
    requests: Vec<u64>,
    /// Told once the held request waits.
    on_hold: Option<oneshot::Sender<()>>,
    /// What the held request waits for.
    released: Option<oneshot::Receiver<()>>,
    /// What the scenario releases the held request by.
    releases: Option<oneshot::Sender<()>>,
}

/// A link from a port of its own to one client endpoint.
#[derive(Debug)]
pub(crate) struct LossyLink {
    address: SocketAddr,
    state: Arc<Mutex<LinkState>>,
}

impl LossyLink {
    /// A link to the client endpoint at `to`, a `tcp://` address, once the system group behind it is past its quiet time: a
    /// link loses a frame so that its client sends again, and the group answers a resend's check 14 while it is quiet.
    #[expect(
        clippy::disallowed_methods,
        reason = "the link carries bytes beside the scenarios, on threads of the test's own"
    )]
    pub(crate) fn to(to: &ClientAt) -> TestResult<Self> {
        crate::cluster::after_quiet_time(to);
        let target: SocketAddr = to
            .address
            .strip_prefix("tcp://")
            .ok_or_else(|| TestError::new(format!("a TCP endpoint, not {}", to.address)))?
            .parse()?;
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        let state = Arc::new(Mutex::new(LinkState::default()));
        let held = Arc::clone(&state);
        drop(std::thread::spawn(move || {
            for accepted in listener.incoming().flatten() {
                // A connection the frontend refuses is what the client then sees: its socket closes.
                drop(carry(accepted, target, &held));
            }
        }));
        Ok(Self { address, state })
    }

    /// Where a client of the link dials.
    pub(crate) fn client_at(&self) -> ClientAt {
        ClientAt::clear_text(&format!("tcp://{}", self.address))
    }

    fn arm(&self, fault: Fault) {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .armed = Some(fault);
    }

    /// The next request of `command` is lost, with its connection.
    pub(crate) fn lose_request(&self, command: Command) {
        self.arm(Fault::LoseRequest(command));
    }

    /// The next reply of `command` is lost, with its connection.
    pub(crate) fn lose_reply(&self, command: Command) {
        self.arm(Fault::LoseReply(command));
    }

    /// The next reply of `command` arrives with bit `at % 8` of its byte `at` flipped.
    pub(crate) fn flip_reply(&self, command: Command, at: usize) {
        self.arm(Fault::FlipReply(command, at));
    }

    /// The request of `command` numbered `nth` from now, from 1, waits in the link until [`LossyLink::release`], and every
    /// frame behind it on its way: the receiver is told once it waits.
    pub(crate) fn hold_request(&self, command: Command, nth: usize) -> oneshot::Receiver<()> {
        let (told, on_hold) = oneshot::channel();
        let (releases, released) = oneshot::channel();
        let mut held = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        held.requests.clear();
        held.on_hold = Some(told);
        held.released = Some(released);
        held.releases = Some(releases);
        held.armed = Some(Fault::HoldRequest(command, nth));
        on_hold
    }

    /// Lets the held request go on, and every frame behind it: one the hold has not reached yet passes when it does.
    pub(crate) fn release(&self) {
        let releases = self
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .releases
            .take();
        if let Some(releases) = releases {
            // A request whose connection closed waits for nothing.
            let _unheld = releases.send(());
        }
    }

    /// The stream of each request of the held command since the hold was armed.
    pub(crate) fn requests(&self) -> Vec<u64> {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .requests
            .clone()
    }

    /// The frames lost so far.
    pub(crate) fn lost(&self) -> Vec<Vec<u8>> {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .lost
            .clone()
    }

    /// The connections carried so far.
    pub(crate) fn connections(&self) -> u32 {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .connections
    }
}

/// Which way a frame travels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Way {
    Request,
    Reply,
}

/// Starts carrying `client`'s connection to `target`, on a thread each way.
#[expect(
    clippy::disallowed_methods,
    reason = "the link carries bytes beside the scenarios, on threads of the test's own"
)]
fn carry(
    client: TcpStream,
    target: SocketAddr,
    state: &Arc<Mutex<LinkState>>,
) -> std::io::Result<()> {
    let server = TcpStream::connect(target)?;
    client.set_nodelay(true)?;
    server.set_nodelay(true)?;
    state
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .connections += 1;
    let (up_from, up_to) = (client.try_clone()?, server.try_clone()?);
    let up_state = Arc::clone(state);
    drop(std::thread::spawn(move || {
        pump(up_from, up_to, &up_state, Way::Request);
    }));
    let down_state = Arc::clone(state);
    drop(std::thread::spawn(move || {
        pump(server, client, &down_state, Way::Reply);
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

/// What the request `frame`, of the command numbered `command`, waits for, when a hold armed for that command reaches it: each
/// request of the command counted by its stream, and the one the hold names told and held.
fn held_until(
    state: &Mutex<LinkState>,
    frame: &[u8],
    command: Option<u16>,
) -> Option<oneshot::Receiver<()>> {
    let mut held = state.lock().unwrap_or_else(PoisonError::into_inner);
    let Some(Fault::HoldRequest(armed, nth)) = held.armed else {
        return None;
    };
    if command != Some(armed.number()) {
        return None;
    }
    let stream = HeaderView::new(frame).map_or(0, |header| header.stream());
    held.requests.push(stream);
    if held.requests.len() < nth {
        return None;
    }
    held.armed = None;
    if let Some(told) = held.on_hold.take() {
        // A scenario that stopped waiting has no one to tell.
        let _untold = told.send(());
    }
    held.released.take()
}

/// The number of a frame's command, from its header.
fn command_of(frame: &[u8]) -> Option<u16> {
    let number = frame.get(54..56)?;
    Some(u16::from_le_bytes(number.try_into().ok()?))
}

/// Carries frames from `from` to `to` until either closes or a frame is lost, then closes both; a held request, and every frame
/// behind it, waits until the scenario releases it.
fn pump(mut from: TcpStream, mut to: TcpStream, state: &Mutex<LinkState>, way: Way) {
    while let Some(mut frame) = next_frame(&mut from) {
        let command = command_of(&frame);
        let matches = |armed: Command| command == Some(armed.number());
        if way == Way::Request
            && let Some(released) = held_until(state, &frame, command)
        {
            // A release dropped unsent lets the frame go as a release does.
            let _released = futures::executor::block_on(released);
        }
        let mut held = state.lock().unwrap_or_else(PoisonError::into_inner);
        let lost = match held.armed {
            Some(Fault::LoseRequest(armed)) => way == Way::Request && matches(armed),
            Some(Fault::LoseReply(armed)) => way == Way::Reply && matches(armed),
            Some(Fault::HoldRequest(..)) => false,
            Some(Fault::FlipReply(armed, at)) => {
                if way == Way::Reply && matches(armed) {
                    // A frame shorter than `at` takes it in its last byte.
                    let at = at.min(frame.len().saturating_sub(1));
                    if let Some(byte) = frame.get_mut(at) {
                        *byte ^= 1 << (at % 8);
                    }
                    held.armed = None;
                }
                false
            }
            None => false,
        };
        if lost {
            held.armed = None;
            held.lost.push(frame);
            break;
        }
        drop(held);
        if to.write_all(&frame).is_err() {
            break;
        }
    }
    drop(from.shutdown(Shutdown::Both));
    drop(to.shutdown(Shutdown::Both));
}
