/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod client_requests;

use std::time::Instant;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use ultragraphdb_client_core::{ClientConfig, ClientCore, ClientError, Pending, Reply};
use ultragraphdb_protocol_transport::{
    BufferPool, FrameStep, Framer, ReceivedFrame, TransportError,
};
use ultragraphdb_protocol_wire::MAX_FRAME_SIZE;

use crate::traits::connect::Connect;

/// The time, as the core is told it: tokio's clock, so a paused runtime moves it.
fn now() -> Instant {
    tokio::time::Instant::now().into_std()
}

/// The next frame of `stream`, read as `framer` asks into a buffer of `pool`: `read_buf` fills the buffer's spare
/// capacity, the pool's, so the bytes land where the frame stays.
async fn read_frame<S: AsyncRead + Unpin>(
    stream: &mut S,
    framer: &mut Framer,
    pool: &BufferPool,
) -> Result<ReceivedFrame, TransportError> {
    let mut buffer = pool.acquire()?;
    framer.reset();
    loop {
        let mut bytes = buffer.take();
        while bytes.len() < framer.want() {
            let room = (framer.want() - bytes.len()) as u64;
            match (&mut *stream).take(room).read_buf(&mut bytes).await {
                Ok(0) => {
                    let received = bytes.len();
                    buffer.restore(bytes);
                    return Err(framer.closed(received));
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => {
                    buffer.restore(bytes);
                    framer.reset();
                    return Err(TransportError::Io(&error));
                }
            }
        }
        buffer.restore(bytes);
        match framer.advance(&buffer) {
            Ok(FrameStep::Read(_)) => {}
            Ok(FrameStep::Frame) => return Ok(framer.finish(buffer)),
            Err(refusal) => {
                framer.reset();
                return Err(refusal);
            }
        }
    }
}

/// The Rust client on tokio: the I/O shell of [`ClientCore`]. It connects through its [`Connect`]or when a request
/// needs a connection, and again whenever the core asks to retry a commit, writes every frame the core queues, reads
/// frames as the transport's framer asks into a pool of frame buffers reused across connections, and gives the core
/// the time and its timeouts. Every operation of the protocol is the core's.
#[derive(Debug)]
pub struct Client<C: Connect> {
    connector: C,
    core: ClientCore,
    pool: BufferPool,
    framer: Framer,
    stream: Option<C::Stream>,
    failure: Option<ClientError>,
}

impl<C: Connect> Client<C> {
    /// A client configured by `config`, connecting through `connector` when first asked; its pool holds
    /// `buffers` frame buffers of 1 MiB, the most frames it holds at once.
    pub fn new(connector: C, config: ClientConfig, buffers: usize) -> Self {
        let framer = Framer::new(config.cluster());
        Self {
            connector,
            core: ClientCore::new(config),
            pool: BufferPool::new(buffers.max(2), MAX_FRAME_SIZE),
            framer,
            stream: None,
            failure: None,
        }
    }

    /// The core, for what the shell does not wrap.
    pub fn core(&self) -> &ClientCore {
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
            .unwrap_or_else(ClientError::ConnectionLost))
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
            Ok(Err(error)) => return Err(ClientError::Transport(TransportError::Io(&error))),
            Err(_) => return Err(ClientError::TimedOut()),
        };
        self.stream = Some(stream);
        self.core.connect(now())
    }

    /// Shuts the connection down, if any.
    async fn shut(&mut self) {
        if let Some(mut stream) = self.stream.take() {
            let _ = stream.shutdown().await;
        }
    }

    /// Writes every frame the core queued; a failure is the connection's.
    async fn flush(&mut self) {
        while let Some(frame) = self.core.transmit() {
            let Some(stream) = self.stream.as_mut() else {
                return;
            };
            let written = match stream.write_all(&frame).await {
                Ok(()) => stream.flush().await,
                Err(error) => Err(error),
            };
            if let Err(error) = written {
                self.core.disconnected(TransportError::Io(&error), now());
                self.stream = None;
                return;
            }
        }
    }

    /// Runs the connection until `done` holds for the core: writes, reads, timeouts, and the reconnects a commit's
    /// retry asks for. Refused when `done` cannot come: nothing is awaited and no commit waits for a connection.
    async fn drive(
        &mut self,
        mut done: impl FnMut(&mut ClientCore) -> bool,
    ) -> Result<(), ClientError> {
        loop {
            if done(&mut self.core) {
                self.flush().await;
                return Ok(());
            }
            if self.core.needs_connection() {
                if let Err(error) = self.open().await {
                    self.core.disconnected(TransportError::Closed(), now());
                    self.failure = Some(error);
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
                    .unwrap_or_else(ClientError::NotConnected));
            };
            let Some(stream) = self.stream.as_mut() else {
                self.core.handle_timeout(deadline);
                continue;
            };
            let read = read_frame(stream, &mut self.framer, &self.pool);
            match tokio::time::timeout_at(deadline.into(), read).await {
                Ok(Ok(frame)) => {
                    if let Err(error) = self.core.receive(frame, now()) {
                        self.failure = Some(error);
                    }
                }
                Ok(Err(error)) => {
                    self.core.disconnected(error.clone(), now());
                    self.failure = Some(ClientError::Transport(error));
                    self.stream = None;
                }
                Err(_) => {
                    // The read was cut where it stood, possibly inside a frame, so the stream cannot be read on: a
                    // connection the core still holds is given up, and a commit waiting is retried on the next.
                    self.core.handle_timeout(now());
                    if self.core.is_connected() {
                        self.core.disconnected(TransportError::Closed(), now());
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
        answer.unwrap_or_else(|| Err(ClientError::ConnectionLost()))
    }

    /// Closes the connection: requests waiting fail, a commit not answered is in doubt.
    pub async fn close(&mut self) {
        self.core.close();
        self.shut().await;
    }
}
