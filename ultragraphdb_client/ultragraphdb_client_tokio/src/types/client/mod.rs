/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod client_requests;

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use ultragraphdb_client_core::{ClientConfig, ClientCore, ClientError, Pending, Reply};
use ultragraphdb_protocol_transport::{
    BufferPool, FrameStep, Framer, Login, PooledBuffer, ReceivedFrame, TransportError,
};
use ultragraphdb_protocol_wire::MAX_FRAME_SIZE;

use crate::traits::connect::Connect;

/// The time, as the core is told it: tokio's clock, so a paused runtime moves it.
fn now() -> Instant {
    tokio::time::Instant::now().into_std()
}

/// The wall clock, microseconds of Unix time, as the core is told it once per connection; 0 before 1970.
#[expect(
    clippy::disallowed_methods,
    reason = "the shell reads the wall clock once per connection and tells the core, which reads none"
)]
fn unix_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_micros() as u64)
}

/// Reads into `bytes` until it holds `want` bytes: `false` when the peer closed first. What a read took stays in `bytes`
/// whatever happens to the future, since `read_buf` either fills or takes nothing.
async fn fill<S: AsyncRead + Unpin>(
    stream: &mut S,
    bytes: &mut Vec<u8>,
    want: usize,
) -> std::io::Result<bool> {
    // Ends: each turn reads at least one byte toward `want`, or returns.
    while bytes.len() < want {
        let room = (want - bytes.len()) as u64;
        match (&mut *stream).take(room).read_buf(bytes).await {
            Ok(0) => return Ok(false),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(true)
}

/// The next frame of `stream`, read as `framer` asks into a buffer of `pool`: `read_buf` fills the buffer's spare
/// capacity, the pool's, so the bytes land where the frame stays. The frame being read is `incoming`, its buffer and the
/// bytes taken out of it, kept by the caller: a read cut at any `.await` loses nothing, and the next call goes on from
/// where it stood. With no buffer free it waits for one, reading nothing, so a full pool holds the server back.
async fn read_frame<S: AsyncRead + Unpin>(
    stream: &mut S,
    framer: &mut Framer,
    pool: &BufferPool,
    incoming: &mut Option<(PooledBuffer, Vec<u8>)>,
) -> Result<ReceivedFrame, TransportError> {
    // Ends: the framer asks for the header and then the frame's size, and a read of no bytes ends the frame.
    loop {
        let (_, into) = match incoming {
            Some(held) => held,
            None => {
                let mut buffer = pool.acquire_waiting().await;
                framer.reset();
                let taken = buffer.take();
                incoming.insert((buffer, taken))
            }
        };
        let filled = fill(stream, into, framer.want()).await;
        // Nothing awaits between here and the frame's next fill, so its buffer is taken out and put back whole.
        let Some((mut buffer, read)) = incoming.take() else {
            continue;
        };
        let received = read.len();
        buffer.restore(read);
        match filled {
            Ok(true) => {}
            Ok(false) => return Err(framer.closed(received)),
            Err(error) => {
                framer.reset();
                return Err(TransportError::io(&error));
            }
        }
        match framer.advance(&buffer) {
            Ok(FrameStep::Read(_)) => {
                let more = buffer.take();
                *incoming = Some((buffer, more));
            }
            Ok(FrameStep::Frame) => return Ok(framer.finish(buffer)),
            Err(refusal) => {
                framer.reset();
                return Err(refusal);
            }
        }
    }
}

/// The frame `incoming` held, given up with its connection: its bytes put back, so the pool takes back its buffer whole.
fn discard(incoming: &mut Option<(PooledBuffer, Vec<u8>)>) {
    if let Some((mut buffer, bytes)) = incoming.take() {
        buffer.restore(bytes);
    }
}

/// The Rust client on tokio: the I/O shell of [`ClientCore`]. It connects through its [`Connect`]or when a request
/// needs a connection, and again whenever the core asks to retry a commit, writes every frame the core queues, reads
/// frames as the transport's framer asks into a pool of frame buffers reused across connections, and gives the core
/// the time and its timeouts. It authenticates through its [`Login`]; a framer is made afresh for each connection,
/// whose frames are numbered from 1. Every operation of the protocol is the core's.
///
/// A caller dropping one of this client's futures at an `.await` leaves the core whole: its requests stay as they
/// stand and answer on the next call, a commit's among them. A frame read in part when its future is dropped, or when a
/// deadline of the core's passes, stays as far as it was read, and the next read goes on from there, so only a real
/// timeout, a failure or a close gives the connection up: a busy commit's resend and a credential's renewal go out on it
/// while its reads go on. The core opens a read only while the pool holds what its credit can bring, and a full pool
/// reads nothing until a frame gives its buffer back.
#[derive(Debug)]
pub struct Client<C: Connect, L: Login> {
    connector: C,
    core: ClientCore<L>,
    pool: BufferPool,
    framer: Framer,
    stream: Option<C::Stream>,
    /// The frame being read, in its buffer, its bytes taken out to be read into.
    incoming: Option<(PooledBuffer, Vec<u8>)>,
    /// The frame being written, and how many of its bytes have left: a write cut at any `.await` goes on from there, so no
    /// other frame follows a part of one.
    outgoing: Option<(Vec<u8>, usize)>,
    failure: Option<ClientError>,
}

impl<C: Connect, L: Login> Client<C, L> {
    /// A client configured by `config`, authenticating through `login`, connecting through `connector` when first
    /// asked; its pool holds `buffers` frame buffers of 1 MiB, at least 2, the most frames it holds at once, which the
    /// core's [`ClientConfig::with_buffers`] is set to.
    #[must_use]
    pub fn new(connector: C, config: ClientConfig, login: L, buffers: usize) -> Self {
        let config = config.with_buffers(buffers);
        let framer = Framer::new(config.cluster(), MAX_FRAME_SIZE);
        Self {
            connector,
            pool: BufferPool::new(config.buffers(), MAX_FRAME_SIZE),
            core: ClientCore::new(config, login),
            framer,
            stream: None,
            incoming: None,
            outgoing: None,
            failure: None,
        }
    }

    /// The core, for what the shell does not wrap.
    #[must_use]
    pub fn core(&self) -> &ClientCore<L> {
        &self.core
    }

    /// Opens a connection and runs the handshake, refused with why it failed.
    pub async fn connect(&mut self) -> Result<(), ClientError> {
        self.open().await?;
        self.drive(|core| core.is_ready() || !core.is_connected())
            .await?;
        if self.core.is_ready() {
            return Ok(());
        }
        Err(self
            .failure
            .take()
            .unwrap_or_else(|| ClientError::connection_lost(None)))
    }

    /// A connection, opened if there is none or the core gave the last up.
    async fn ready(&mut self) -> Result<(), ClientError> {
        if self.core.is_ready() && self.stream.is_some() {
            return Ok(());
        }
        self.connect().await
    }

    async fn open(&mut self) -> Result<(), ClientError> {
        self.shut().await;
        let timeout = self.core.config().timeout();
        let stream = match tokio::time::timeout(timeout, self.connector.connect()).await {
            Ok(Ok(stream)) => stream,
            Ok(Err(error)) => return Err(ClientError::transport(TransportError::io(&error))),
            Err(_) => return Err(ClientError::timed_out()),
        };
        self.stream = Some(stream);
        discard(&mut self.incoming);
        self.outgoing = None;
        self.framer = Framer::new(self.core.config().cluster(), MAX_FRAME_SIZE);
        self.core.connect(now(), unix_micros())
    }

    /// Shuts the connection down, if any.
    async fn shut(&mut self) {
        discard(&mut self.incoming);
        self.outgoing = None;
        if let Some(mut stream) = self.stream.take() {
            // The connection is given up, so a failed shutdown, or one the peer does not take within the timeout, is dropped.
            drop(tokio::time::timeout(self.core.config().timeout(), stream.shutdown()).await);
        }
    }

    /// Writes the frame cut short, if any, then every frame the core queued, each within the timeout; a failure is the
    /// connection's, and the shell's answer when nothing else is. A peer that takes no frame within the timeout has stopped
    /// reading, and the write times out. Each write counts the bytes that left, so a future dropped mid-frame leaves the
    /// rest for the next call.
    async fn flush(&mut self) {
        let timeout = self.core.config().timeout();
        // Ends: each turn writes one frame whole, or gives the connection up, and the core's queue is finite.
        loop {
            if self.outgoing.is_none() {
                let Some(frame) = self.core.transmit() else {
                    return;
                };
                self.outgoing = Some((frame, 0));
            }
            let (Some(stream), Some((frame, at))) = (self.stream.as_mut(), self.outgoing.as_mut())
            else {
                return;
            };
            let writing = async {
                // Ends: each turn writes at least one byte, or fails.
                while let Some(rest) = frame.get(*at..).filter(|rest| !rest.is_empty()) {
                    let wrote = stream.write(rest).await?;
                    if wrote == 0 {
                        return Err(std::io::ErrorKind::WriteZero.into());
                    }
                    *at += wrote;
                }
                stream.flush().await
            };
            let written = tokio::time::timeout(timeout, writing)
                .await
                .unwrap_or_else(|_| Err(std::io::ErrorKind::TimedOut.into()));
            self.outgoing = None;
            if let Err(error) = written {
                let error = TransportError::io(&error);
                self.core.disconnected(error.clone(), now());
                self.failure = Some(ClientError::transport(error));
                self.stream = None;
                return;
            }
        }
    }

    /// Hands the core `frame`, read on the connection.
    fn take_frame(&mut self, frame: ReceivedFrame) {
        if let Err(error) = self.core.receive(frame, now()) {
            self.failure = Some(error);
        }
        if let Some(welcome) = self.core.welcome() {
            self.framer.set_protocol(welcome.protocol());
        }
    }

    /// The frames that have already arrived, read before a deadline that passed while no call drove the connection is
    /// taken, so a read whose frame waits in the socket is not taken for a silent one; at most the pool's buffers, so a
    /// deadline due is not put off by frames that keep coming.
    async fn take_arrived(&mut self) {
        for _ in 0..self.core.config().buffers() {
            let Some(stream) = self.stream.as_mut() else {
                return;
            };
            let read = read_frame(stream, &mut self.framer, &self.pool, &mut self.incoming);
            // A timeout of no time polls the read once: a frame already in the socket is read, a frame in part stays.
            match tokio::time::timeout(std::time::Duration::ZERO, read).await {
                Ok(Ok(frame)) => self.take_frame(frame),
                Ok(Err(error)) => {
                    self.core.disconnected(error.clone(), now());
                    self.failure = Some(ClientError::transport(error));
                    self.stream = None;
                    return;
                }
                Err(_) => return,
            }
        }
    }

    /// Runs the connection until `done` holds for the core: writes, reads, timeouts, and the reconnects a commit's
    /// retry asks for, `retry_delay` apart after a connection not made or lost in its handshake, each connection not
    /// made counted against the commit's attempts. Refused when `done`
    /// cannot come: nothing is awaited and no commit waits for a connection.
    async fn drive(
        &mut self,
        mut done: impl FnMut(&mut ClientCore<L>) -> bool,
    ) -> Result<(), ClientError> {
        // Event loop: it ends when `done` holds or the core has nothing left to wait for, each turn a write, a read or a
        // timeout, every wait bounded by the core's next deadline and every reconnect by a commit's attempts.
        loop {
            if done(&mut self.core) {
                self.flush().await;
                return Ok(());
            }
            if self.core.needs_connection() {
                if self.core.handshake_lost() {
                    // The last connection was lost in its handshake: the next waits the retry delay, as after a
                    // connection not made.
                    tokio::time::sleep(self.core.config().retry_delay()).await;
                }
                if let Err(error) = self.open().await {
                    // The failed attempt counts against the waiting commits' attempts; the next waits the retry delay
                    // on the runtime's timer, so the loop yields and an outer timeout can cancel it.
                    self.core.connect_failed(&error);
                    self.failure = Some(error);
                    if self.core.needs_connection() {
                        tokio::time::sleep(self.core.config().retry_delay()).await;
                    }
                }
                continue;
            }
            self.flush().await;
            if done(&mut self.core) {
                return Ok(());
            }
            if !self.core.is_connected() {
                self.shut().await;
            }
            let Some(deadline) = self.core.next_deadline() else {
                return Err(self
                    .failure
                    .take()
                    .unwrap_or_else(ClientError::not_connected));
            };
            let Some(stream) = self.stream.as_mut() else {
                self.core.handle_timeout(deadline);
                continue;
            };
            let read = read_frame(stream, &mut self.framer, &self.pool, &mut self.incoming);
            match tokio::time::timeout_at(deadline.into(), read).await {
                Ok(Ok(frame)) => {
                    self.take_frame(frame);
                    // A deadline passed by the time the frame was read is taken once the frames already arrived are: a resend
                    // or a renewal due goes out, and a timeout counts from the frames' progress.
                    if self.core.next_deadline().is_some_and(|due| due <= now()) {
                        self.take_arrived().await;
                        self.core.handle_timeout(now());
                        if !self.core.is_connected() {
                            self.shut().await;
                        }
                    }
                }
                Ok(Err(error)) => {
                    self.core.disconnected(error.clone(), now());
                    self.failure = Some(ClientError::transport(error));
                    self.stream = None;
                }
                Err(_) => {
                    // A deadline passed: the frame being read stays as far as it was read, and the next turn reads on.
                    // The core decides what the deadline was; only a real timeout gives the connection up.
                    self.core.handle_timeout(now());
                    if !self.core.is_connected() {
                        self.shut().await;
                    }
                }
            }
        }
    }

    /// The answer of `pending`, once it arrives.
    async fn settle(&mut self, pending: Pending) -> Result<Reply, ClientError> {
        let mut answer = None;
        self.drive(|core| {
            answer = core.take_reply(pending);
            answer.is_some()
        })
        .await?;
        answer.unwrap_or_else(|| Err(ClientError::connection_lost(None)))
    }

    /// Closes the connection: requests waiting fail, a commit not answered is in doubt.
    pub async fn close(&mut self) {
        self.core.close();
        self.shut().await;
    }
}
