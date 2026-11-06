/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod client_requests;

use std::future::poll_fn;
use std::pin::{Pin, pin};
use std::task::Poll;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use compio::io::util::Splittable;
use compio::runtime::JoinHandle;
use ultragraphdb_client_core::{ClientConfig, ClientCore, ClientError, Pending, Reply};
use ultragraphdb_protocol_transport::{BufferPool, Framer, Login, ReceivedFrame, TransportError};
use ultragraphdb_protocol_transport_compio::Connection;
use ultragraphdb_protocol_wire::MAX_FRAME_SIZE;

use crate::traits::connect::Connect;
use crate::traits::split_stream::SplitStream;

/// The receiving half of a connection of `C`.
type Reader<C> = Connection<<<C as Connect>::Stream as SplitStream>::Read>;

/// The sending half of a connection of `C`.
type Writer<C> = Connection<<<C as Connect>::Stream as SplitStream>::Write>;

/// A frame being read on a task of its own, which hands back the receiving half with what it read.
type Reading<C> = JoinHandle<(Reader<C>, Result<ReceivedFrame, TransportError>)>;

/// A frame being written on a task of its own, which hands back the sending half with how the write went.
type Writing<C> = JoinHandle<(Writer<C>, Result<(), TransportError>)>;

/// How long a read has before a deadline already passed when the wait begins is taken: a frame that arrived while no call
/// drove the connection is read first, its task given its turns on the runtime, so the core never takes a server that
/// answered for a silent one.
const READ_TURN: Duration = Duration::from_millis(10);

/// What `reading` read by `until`, `None` past it; the read is asked first, so a frame read is taken whatever the time.
async fn read_by<F: Future + Unpin>(reading: &mut F, until: Instant) -> Option<F::Output> {
    let mut timer = pin!(compio::time::sleep_until(until));
    poll_fn(|cx| {
        if let Poll::Ready(read) = Pin::new(&mut *reading).poll(cx) {
            return Poll::Ready(Some(read));
        }
        timer.as_mut().poll(cx).map(|()| None)
    })
    .await
}

/// The time, as the core is told it: the shell is where the client reads the clock, and the core is given it.
#[expect(
    clippy::disallowed_methods,
    reason = "the shell reads the clock and tells the core, which reads none"
)]
fn now() -> Instant {
    Instant::now()
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

/// The Rust client on compio: the I/O shell of [`ClientCore`]. It connects through its [`Connect`]or when a request
/// needs a connection, and again whenever the core asks to retry a commit, writes every frame the core queues on the
/// connection's sending half, reads frames through the transport's compio [`Connection`] on its receiving half into a
/// pool of frame buffers reused across connections, and gives the core the time and its timeouts. It authenticates
/// through its [`Login`]. Every operation of the protocol is the core's.
///
/// Each frame is read on a task of its own on the runtime the client runs on, so no read is ever cut: a deadline of
/// the core's passing, or a caller dropping one of this client's futures at an `.await`, leaves the read going on, and
/// the next call takes its frame, read before any deadline that passed meanwhile is taken. Each frame is written on a task of
/// its own too, so a caller's future dropped mid-frame leaves the write going on, and the next frame follows it whole. Only a
/// real timeout, a failure or a close gives the connection up: a busy commit's
/// resend and a credential's renewal go out on the sending half while a read is out. The core's requests stay as they
/// stand and answer on the next call, a commit's among them. The core opens a read only while the pool holds what its
/// credit can bring, and a full pool reads nothing until a frame gives its buffer back.
#[derive(Debug)]
pub struct Client<C: Connect, L: Login> {
    connector: C,
    core: ClientCore<L>,
    pool: BufferPool,
    /// The sending half while no frame is being written on it.
    writer: Option<Writer<C>>,
    /// The frame being written, on its task: a caller dropping a future mid-frame leaves it going on, so no other frame
    /// follows a part of one.
    writing: Option<Writing<C>>,
    /// The receiving half while no frame is being read on it.
    reader: Option<Reader<C>>,
    /// The frame being read, on its task.
    reading: Option<Reading<C>>,
    failure: Option<ClientError>,
}

impl<C: Connect, L: Login> Client<C, L> {
    /// A client configured by `config`, authenticating through `login`, connecting through `connector` when first
    /// asked; its pool holds `buffers` frame buffers of 1 MiB, at least 2, the most frames it holds at once, which the
    /// core's [`ClientConfig::with_buffers`] is set to.
    #[must_use]
    pub fn new(connector: C, config: ClientConfig, login: L, buffers: usize) -> Self {
        let config = config.with_buffers(buffers);
        Self {
            connector,
            pool: BufferPool::new(config.buffers(), MAX_FRAME_SIZE),
            core: ClientCore::new(config, login),
            writer: None,
            writing: None,
            reader: None,
            reading: None,
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
        if self.core.is_ready() && (self.writer.is_some() || self.writing.is_some()) {
            return Ok(());
        }
        self.connect().await
    }

    async fn open(&mut self) -> Result<(), ClientError> {
        self.shut().await;
        let timeout = self.core.config().timeout();
        let stream = match compio::time::timeout(timeout, self.connector.connect()).await {
            Ok(Ok(stream)) => stream,
            Ok(Err(error)) => return Err(ClientError::transport(TransportError::io(&error))),
            Err(_) => return Err(ClientError::timed_out()),
        };
        let framer = Framer::new(self.core.config().cluster(), MAX_FRAME_SIZE);
        let (read, write) = stream.split();
        self.reader = Some(Connection::new(read, framer));
        self.writer = Some(Connection::new(write, framer));
        self.core.connect(now(), unix_micros())
    }

    /// Shuts the connection down, if any: a frame being read is given up with it, and a shutdown the peer does not take
    /// within the timeout is dropped.
    async fn shut(&mut self) {
        self.reading = None;
        self.reader = None;
        self.writing = None;
        if let Some(mut writer) = self.writer.take() {
            let _shut: Result<(), compio::time::Elapsed> =
                compio::time::timeout(self.core.config().timeout(), writer.shutdown()).await;
        }
    }

    /// The connection is gone: its halves and a frame being read with it.
    fn drop_connection(&mut self) {
        self.reading = None;
        self.reader = None;
        self.writing = None;
        self.writer = None;
    }

    /// Writes the frame being written, if any, then every frame the core queued, each on a task of its own and within the
    /// timeout; a failure is the connection's, and the shell's answer when nothing else is. A peer that takes no frame within
    /// the timeout has stopped reading, and the write times out.
    async fn flush(&mut self) {
        let timeout = self.core.config().timeout();
        // Ends: each turn writes one frame whole, or gives the connection up, and the core's queue is finite.
        loop {
            if self.writing.is_none() {
                let Some(mut writer) = self.writer.take() else {
                    return;
                };
                let Some(frame) = self.core.transmit() else {
                    self.writer = Some(writer);
                    return;
                };
                self.writing = Some(compio::runtime::spawn(async move {
                    let written = writer.write_frame(frame).await.map(|_sent| ());
                    (writer, written)
                }));
            }
            let Some(writing) = self.writing.as_mut() else {
                return;
            };
            let written = compio::time::timeout(timeout, writing).await;
            self.writing = None;
            let failed = match written {
                Ok(Ok((writer, Ok(())))) => {
                    self.writer = Some(writer);
                    continue;
                }
                Ok(Ok((_, Err(error)))) => error,
                Ok(Err(ended)) => TransportError::io(&std::io::Error::from(ended)),
                Err(_) => TransportError::io(&std::io::ErrorKind::TimedOut.into()),
            };
            self.core.disconnected(failed.clone(), now());
            self.failure = Some(ClientError::transport(failed));
            self.drop_connection();
            return;
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
                    compio::time::sleep(self.core.config().retry_delay()).await;
                }
                if let Err(error) = self.open().await {
                    // The failed attempt counts against the waiting commits' attempts; the next waits the retry delay
                    // on the runtime's timer, so the loop yields and an outer timeout can cancel it.
                    self.core.connect_failed(&error);
                    self.failure = Some(error);
                    if self.core.needs_connection() {
                        compio::time::sleep(self.core.config().retry_delay()).await;
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
            let Some(reading) = self.read_out() else {
                self.core.handle_timeout(deadline);
                continue;
            };
            let at = now();
            let until = if deadline > at {
                deadline
            } else {
                at.checked_add(READ_TURN).unwrap_or(deadline)
            };
            match read_by(reading, until).await {
                Some(read) => {
                    self.reading = None;
                    self.take_read(read);
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
                None => {
                    // A deadline passed: the frame being read goes on on its task. The core decides what the deadline
                    // was; only a real timeout gives the connection up.
                    self.core.handle_timeout(now());
                    if !self.core.is_connected() {
                        self.shut().await;
                    }
                }
            }
        }
    }

    /// The frames that have already arrived, each given [`READ_TURN`], read before a deadline that passed while no call drove
    /// the connection is taken, so a read whose frame waits in the socket is not taken for a silent one; at most the pool's
    /// buffers, so a deadline due is not put off by frames that keep coming.
    async fn take_arrived(&mut self) {
        for _ in 0..self.core.config().buffers() {
            let until = now().checked_add(READ_TURN).unwrap_or_else(now);
            let Some(reading) = self.read_out() else {
                return;
            };
            let Some(read) = read_by(reading, until).await else {
                return;
            };
            self.reading = None;
            self.take_read(read);
        }
    }

    /// The frame being read, its read started on a task of its own if none is out: the task holds the receiving half and
    /// a buffer of the pool, waiting for one while none is free, and hands the half back with what it read.
    fn read_out(&mut self) -> Option<&mut Reading<C>> {
        if self.reading.is_none() {
            let mut reader = self.reader.take()?;
            let pool = self.pool.clone();
            self.reading = Some(compio::runtime::spawn(async move {
                let read = reader.read_frame(&pool).await;
                (reader, read)
            }));
        }
        self.reading.as_mut()
    }

    /// What a frame's task read: the frame to the core, its half kept for the next; a failure, or a task that ended
    /// without its answer, the connection's.
    fn take_read(
        &mut self,
        read: Result<
            (Reader<C>, Result<ReceivedFrame, TransportError>),
            compio::runtime::JoinError,
        >,
    ) {
        let failed = match read {
            Ok((mut reader, Ok(frame))) => {
                if let Err(error) = self.core.receive(frame, now()) {
                    self.failure = Some(error);
                }
                if let Some(welcome) = self.core.welcome() {
                    reader.framer_mut().set_protocol(welcome.protocol());
                }
                self.reader = Some(reader);
                return;
            }
            Ok((_, Err(error))) => error,
            Err(ended) => TransportError::io(&std::io::Error::from(ended)),
        };
        self.core.disconnected(failed.clone(), now());
        self.failure = Some(ClientError::transport(failed));
        self.drop_connection();
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
