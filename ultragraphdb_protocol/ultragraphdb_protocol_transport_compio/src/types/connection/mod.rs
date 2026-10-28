/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::BufResult;
use compio::buf::{IntoInner, IoBuf};
use compio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use ultragraphdb_protocol_transport::{
    BufferPool, ClientHandshake, FrameStep, Framer, ReceivedFrame, ServerHandshake, TransportError,
};
use ultragraphdb_protocol_wire::{Hello, Welcome};

/// How filling a buffer to a length ended.
enum Fill {
    Done,
    /// The peer closed; the bytes filled are the buffer's.
    Closed,
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
            Ok(0) => return (Fill::Closed, bytes),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return (Fill::Failed(e), bytes),
        }
    }
    (Fill::Done, bytes)
}

/// A connection of the binary protocol over a compio stream: a TCP stream, or a [`crate::CompioSimStream`] in the
/// simulator. The I/O shell of the transport's machines: it reads each frame as its [`Framer`] asks, into a pooled
/// buffer the frame stays in, writes frames whole, and runs the handshakes' machines; streams are
/// [`ultragraphdb_protocol_transport::Outbound`]'s.
#[derive(Debug)]
pub struct Connection<S> {
    stream: S,
    framer: Framer,
}

impl<S: AsyncRead + AsyncWrite> Connection<S> {
    /// A connection over `stream` to or from a node of `cluster`.
    pub fn new(stream: S, cluster: u64) -> Self {
        Self {
            stream,
            framer: Framer::new(cluster),
        }
    }

    pub fn cluster(&self) -> u64 {
        self.framer.cluster()
    }

    /// The stream, for its own operations.
    pub fn stream_mut(&mut self) -> &mut S {
        &mut self.stream
    }

    pub fn into_inner(self) -> S {
        self.stream
    }

    /// The next frame, read into a buffer of `pool` as the [`Framer`] asks: its header, verified for the
    /// connection's cluster so its `size` is trusted, then the rest of the frame into the same buffer. Refused as
    /// [`TransportError::Closed`] when the peer closed before the frame's first byte, [`TransportError::Truncated`]
    /// when it closed inside the frame, [`TransportError::Protocol`] when the header breaks a rule or the frame is
    /// larger than the pool's buffers ([`ultragraphdb_error_protocol::ProtocolError::FrameTooLarge`] naming their
    /// capacity), [`TransportError::PoolExhausted`] with no buffer free, or [`TransportError::Io`].
    pub async fn read_frame(&mut self, pool: &BufferPool) -> Result<ReceivedFrame, TransportError> {
        let mut buffer = pool.acquire()?;
        self.framer.reset();
        loop {
            let mut bytes = buffer.take();
            let (filled, rest) = fill(&mut self.stream, bytes, self.framer.want()).await;
            bytes = rest;
            let received = bytes.len();
            buffer.restore(bytes);
            match filled {
                Fill::Done => {}
                Fill::Closed => return Err(self.framer.closed(received)),
                Fill::Failed(error) => {
                    self.framer.reset();
                    return Err(TransportError::Io(&error));
                }
            }
            match self.framer.advance(&buffer) {
                Ok(FrameStep::Read(_)) => {}
                Ok(FrameStep::Frame) => return Ok(self.framer.finish(buffer)),
                Err(refusal) => {
                    self.framer.reset();
                    return Err(refusal);
                }
            }
        }
    }

    /// Writes `frame` whole and flushes it; [`TransportError::Io`] when the stream fails. The frame is handed
    /// back, so its buffer can be reused.
    pub async fn write_frame(&mut self, frame: Vec<u8>) -> Result<Vec<u8>, TransportError> {
        let BufResult(written, frame) = self.stream.write_all(frame).await;
        written.map_err(|e| TransportError::Io(&e))?;
        self.stream
            .flush()
            .await
            .map_err(|e| TransportError::Io(&e))?;
        Ok(frame)
    }

    /// Shuts down the connection's sending side; a failure changes nothing, since the connection is given up.
    pub async fn shutdown(&mut self) {
        let _ = self.stream.shutdown().await;
    }

    /// The client's handshake, run through [`ClientHandshake`]: sends `hello` on stream 0 and reads the server's
    /// answer, its Welcome, refused as [`ClientHandshake::receive`] refuses it.
    pub async fn hello(
        &mut self,
        pool: &BufferPool,
        hello: &Hello,
    ) -> Result<Welcome, TransportError> {
        let handshake = ClientHandshake::new(self.cluster(), hello.clone());
        self.write_frame(handshake.hello_frame()?).await?;
        let answer = self.read_frame(pool).await?;
        handshake.receive(answer.bytes())
    }

    /// The server's handshake, run through [`ServerHandshake`]: reads the client's Hello and sends the answer the
    /// machine decides, returning the Welcome sent or why the handshake failed.
    pub async fn welcome(
        &mut self,
        pool: &BufferPool,
        server: &Welcome,
    ) -> Result<Welcome, TransportError> {
        let received = self.read_frame(pool).await?;
        let (frame, outcome) = ServerHandshake::new(self.cluster(), server.clone())
            .receive(received.bytes())
            .into_parts();
        drop(received);
        match (frame, &outcome) {
            (Some(frame), Ok(_)) => {
                self.write_frame(frame).await?;
            }
            (Some(frame), Err(_)) => {
                let _ = self.write_frame(frame).await;
            }
            (None, _) => {}
        }
        outcome
    }
}
