/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod client_requests;

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use ultragraphdb_client_core::{ClientConfig, ClientCore, ClientError, Pending, Reply};
use ultragraphdb_protocol_transport::{BufferPool, Framer, Login, TransportError};
use ultragraphdb_protocol_transport_compio::Connection;
use ultragraphdb_protocol_wire::MAX_FRAME_SIZE;

use crate::traits::connect::Connect;

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
/// needs a connection, and again whenever the core asks to retry a commit, writes every frame the core queues, reads
/// frames through the transport's compio [`Connection`] into a pool of frame buffers reused across connections, and
/// gives the core the time and its timeouts. It authenticates through its [`Login`]. Every operation of the protocol
/// is the core's.
///
/// A caller dropping one of this client's futures at an `.await` leaves the core whole: its requests stay as they
/// stand and answer on the next call, a commit's among them. A read of the connection cut inside a frame leaves the
/// stream between frames no more, and the next read refuses what it finds, which gives the connection up and retries
/// a commit waiting on the next, as the protocol's own loss does.
#[derive(Debug)]
pub struct Client<C: Connect, L: Login> {
    connector: C,
    core: ClientCore<L>,
    pool: BufferPool,
    connection: Option<Connection<C::Stream>>,
    failure: Option<ClientError>,
}

impl<C: Connect, L: Login> Client<C, L> {
    /// A client configured by `config`, authenticating through `login`, connecting through `connector` when first
    /// asked; its pool holds `buffers` frame buffers of 1 MiB, the most frames it holds at once.
    #[must_use]
    pub fn new(connector: C, config: ClientConfig, login: L, buffers: usize) -> Self {
        Self {
            connector,
            core: ClientCore::new(config, login),
            pool: BufferPool::new(buffers.max(2), MAX_FRAME_SIZE),
            connection: None,
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
        if self.core.is_ready() && self.connection.is_some() {
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
        self.connection = Some(Connection::new(
            stream,
            Framer::new(self.core.config().cluster(), MAX_FRAME_SIZE),
        ));
        self.core.connect(now(), unix_micros())
    }

    /// Shuts the connection down, if any.
    async fn shut(&mut self) {
        if let Some(mut connection) = self.connection.take() {
            connection.shutdown().await;
        }
    }

    /// Writes every frame the core queued; a failure is the connection's, and the shell's answer when nothing else is.
    async fn flush(&mut self) {
        while let Some(frame) = self.core.transmit() {
            let Some(connection) = self.connection.as_mut() else {
                return;
            };
            if let Err(error) = connection.write_frame(frame).await {
                self.core.disconnected(error.clone(), now());
                self.failure = Some(ClientError::transport(error));
                self.connection = None;
                return;
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
            let Some(connection) = self.connection.as_mut() else {
                self.core.handle_timeout(deadline);
                continue;
            };
            let read = connection.read_frame(&self.pool);
            match compio::time::timeout_at(deadline, read).await {
                Ok(Ok(frame)) => {
                    if let Err(error) = self.core.receive(frame, now()) {
                        self.failure = Some(error);
                    }
                    if let (Some(welcome), Some(read_on)) =
                        (self.core.welcome(), self.connection.as_mut())
                    {
                        read_on.framer_mut().set_protocol(welcome.protocol());
                    }
                }
                Ok(Err(error)) => {
                    self.core.disconnected(error.clone(), now());
                    self.failure = Some(ClientError::transport(error));
                    self.connection = None;
                }
                Err(_) => {
                    // The read was cut where it stood, possibly inside a frame, so the stream cannot be read on: a
                    // connection the core still holds is given up, and a commit waiting is retried on the next.
                    self.core.handle_timeout(now());
                    if self.core.is_connected() {
                        self.core.disconnected(TransportError::closed(), now());
                    }
                    self.shut().await;
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
