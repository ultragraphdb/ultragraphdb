/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::BufResult;
use compio::buf::{IntoInner, IoBuf};
use compio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, ErrorReply, ErrorReplyView, HEADER_SIZE, Header, HeaderView,
    Hello, HelloView, Welcome, WelcomeView,
};

use crate::errors::bus_error::BusError;
use crate::types::buffer_pool::BufferPool;
use crate::types::received_frame::ReceivedFrame;

/// How filling a buffer to a length ended.
enum Fill {
    Done,
    /// The peer closed after this many bytes of the buffer were filled.
    Closed(usize),
    Failed(std::io::Error),
}

/// Reads into `bytes` until it holds `target` bytes; the bytes are returned whatever the outcome.
async fn fill<S: AsyncRead>(stream: &mut S, mut bytes: Vec<u8>, target: usize) -> (Fill, Vec<u8>) {
    if bytes.capacity() < target {
        bytes.reserve_exact(target - bytes.len());
    }
    while bytes.len() < target {
        let start = bytes.len();
        let BufResult(read, slice) = stream.read(bytes.slice(start..target)).await;
        bytes = slice.into_inner();
        match read {
            Ok(0) => return (Fill::Closed(bytes.len()), bytes),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return (Fill::Failed(e), bytes),
        }
    }
    (Fill::Done, bytes)
}

/// A connection of the binary protocol over a compio stream: a TCP stream, or a [`crate::SimStream`] in the
/// simulator. It reads whole frames into pooled buffers and writes frames whole; streams are [`crate::Outbound`]'s.
#[derive(Debug)]
pub struct Connection<S> {
    stream: S,
    cluster: u64,
}

impl<S: AsyncRead + AsyncWrite> Connection<S> {
    /// A connection over `stream` to or from a node of `cluster`.
    pub fn new(stream: S, cluster: u64) -> Self {
        Self { stream, cluster }
    }

    pub fn cluster(&self) -> u64 {
        self.cluster
    }

    /// The stream, for its own operations.
    pub fn stream_mut(&mut self) -> &mut S {
        &mut self.stream
    }

    pub fn into_inner(self) -> S {
        self.stream
    }

    /// The next frame, read into a buffer of `pool`: its 256-byte header first, verified for the connection's cluster
    /// so its `size` is trusted, then the rest of the frame into the same buffer. Refused as [`BusError::Closed`] when
    /// the peer closed before the frame's first byte, [`BusError::Truncated`] when it closed inside the frame,
    /// [`BusError::Protocol`] when the header breaks a rule or the frame is larger than the pool's buffers
    /// ([`ultragraphdb_error_protocol::ProtocolError::FrameTooLarge`] naming their capacity),
    /// [`BusError::PoolExhausted`] with no buffer free, or [`BusError::Io`].
    pub async fn read_frame(&mut self, pool: &BufferPool) -> Result<ReceivedFrame, BusError> {
        let mut buffer = pool.acquire()?;
        let mut bytes = buffer.take();
        bytes.clear();
        let (filled, mut bytes) = fill(&mut self.stream, bytes, HEADER_SIZE).await;
        let header = match filled {
            Fill::Done => HeaderView::new(&bytes)
                .and_then(|view| view.verify(self.cluster).map(|_| view.size() as usize)),
            Fill::Closed(0) => {
                buffer.restore(bytes);
                return Err(BusError::Closed());
            }
            Fill::Closed(received) => {
                buffer.restore(bytes);
                return Err(BusError::Truncated(received, HEADER_SIZE));
            }
            Fill::Failed(error) => {
                buffer.restore(bytes);
                return Err(BusError::Io(&error));
            }
        };
        let size = match header {
            Ok(size) if size > pool.capacity() => {
                buffer.restore(bytes);
                return Err(BusError::Protocol(ProtocolError::FrameTooLarge(
                    size,
                    pool.capacity(),
                )));
            }
            Ok(size) => size,
            Err(refusal) => {
                buffer.restore(bytes);
                return Err(BusError::Protocol(refusal));
            }
        };
        if size > HEADER_SIZE {
            let (filled, rest) = fill(&mut self.stream, bytes, size).await;
            bytes = rest;
            match filled {
                Fill::Done => {}
                Fill::Closed(received) => {
                    buffer.restore(bytes);
                    return Err(BusError::Truncated(received, size));
                }
                Fill::Failed(error) => {
                    buffer.restore(bytes);
                    return Err(BusError::Io(&error));
                }
            }
        }
        buffer.restore(bytes);
        Ok(ReceivedFrame::new(buffer, self.cluster))
    }

    /// Writes `frame` whole and flushes it; [`BusError::Io`] when the stream fails.
    pub async fn write_frame(&mut self, frame: Vec<u8>) -> Result<(), BusError> {
        let BufResult(written, _) = self.stream.write_all(frame).await;
        written.map_err(|e| BusError::Io(&e))?;
        self.stream.flush().await.map_err(|e| BusError::Io(&e))
    }

    /// The client's handshake: sends `hello` on stream 0 and reads the answer, the server's Welcome, refused as
    /// [`BusError::Refused`] when it answers with an Error frame, [`BusError::UnexpectedCommand`] for any other
    /// command, and [`BusError::Protocol`] when the Welcome breaks a rule or chose a protocol outside `hello`'s range.
    pub async fn hello(&mut self, pool: &BufferPool, hello: &Hello) -> Result<Welcome, BusError> {
        let header = Header::new(self.cluster, Command::Hello).with_release(hello.release());
        let frame = hello.frame(header).map_err(BusError::Protocol)?;
        self.write_frame(frame).await?;
        let answer = self.read_frame(pool).await?;
        let view = answer.view().map_err(BusError::Protocol)?;
        match view.command() {
            Command::Welcome => {
                let welcome = WelcomeView::from_frame(&view)
                    .and_then(|w| w.to_body())
                    .map_err(BusError::Protocol)?;
                if !(hello.protocol_min()..=hello.protocol_max()).contains(&welcome.protocol()) {
                    return Err(BusError::Protocol(ProtocolError::FieldValue(
                        Command::Welcome.number(),
                        "protocol",
                        welcome.protocol().into(),
                    )));
                }
                Ok(welcome)
            }
            Command::Error => {
                let error = ErrorReplyView::from_frame(&view).map_err(BusError::Protocol)?;
                Err(BusError::Refused(error.code(), error.message()))
            }
            other => Err(BusError::UnexpectedCommand(
                Command::Welcome.number(),
                other.number(),
            )),
        }
    }

    /// Answers with `error` on stream 0 before the connection is closed; a failure to send it changes nothing.
    async fn refuse(&mut self, error: &ErrorReply, header: Header) {
        if let Ok(frame) = error.frame(header) {
            let _ = self.write_frame(frame).await;
        }
    }

    /// The server's handshake: reads the client's Hello and answers it as `server` does ([`Welcome::answer`]),
    /// returning the Welcome sent. A Hello outside the server's protocols is answered with its Error frame and
    /// refused as [`BusError::Refused`]; another command as [`BusError::UnexpectedCommand`], answered with a protocol
    /// Error frame.
    pub async fn welcome(
        &mut self,
        pool: &BufferPool,
        server: &Welcome,
    ) -> Result<Welcome, BusError> {
        let received = self.read_frame(pool).await?;
        let header = Header::new(self.cluster, Command::Welcome).with_release(server.release());
        let view = match received.view() {
            Ok(view) => view,
            Err(refusal) => {
                self.refuse(&ErrorReply::protocol(&refusal), header).await;
                return Err(BusError::Protocol(refusal));
            }
        };
        if view.command() != Command::Hello {
            let found = view.command().number();
            let refusal = ProtocolError::WrongCommand(Command::Hello.number(), found);
            self.refuse(&ErrorReply::protocol(&refusal), header).await;
            return Err(BusError::UnexpectedCommand(Command::Hello.number(), found));
        }
        let hello = match HelloView::from_frame(&view) {
            Ok(hello) => hello,
            Err(refusal) => {
                self.refuse(&ErrorReply::protocol(&refusal), header).await;
                return Err(BusError::Protocol(refusal));
            }
        };
        match server.answer(&hello) {
            Ok(welcome) => {
                let frame = welcome.frame(header).map_err(BusError::Protocol)?;
                self.write_frame(frame).await?;
                Ok(welcome)
            }
            Err(error) => {
                self.refuse(&error, header).await;
                Err(BusError::Refused(error.code(), error.message()))
            }
        }
    }
}
