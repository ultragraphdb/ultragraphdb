/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;
use std::io;
use std::rc::Rc;

use compio::BufResult;
use compio::buf::{IntoInner, IoBuf, IoBufMut};
use compio::io::util::Splittable;
use compio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use ultragraphdb_transport_tls::{Plaintext, READ_TURNS_MAX, RECORD_SIZE_MAX, TlsSession};

use super::CompioTlsStream;

/// A TLS connection's receiving half: a read pending on its stream holds no borrow of the session, so the sending half
/// goes on. What the session comes to owe the peer while reading, an answer to a key update, leaves with the next write.
#[derive(Debug)]
pub struct TlsReadHalf<R> {
    session: Rc<RefCell<TlsSession>>,
    stream: R,
    incoming: Vec<u8>,
    plaintext: Vec<u8>,
}

/// A TLS connection's sending half, whose session its receiving half shares on the same core.
#[derive(Debug)]
pub struct TlsWriteHalf<W> {
    session: Rc<RefCell<TlsSession>>,
    stream: W,
    outgoing: Vec<u8>,
}

impl<S> Splittable for CompioTlsStream<S>
where
    S: Splittable,
{
    type ReadHalf = TlsReadHalf<S::ReadHalf>;
    type WriteHalf = TlsWriteHalf<S::WriteHalf>;

    fn split(self) -> (Self::ReadHalf, Self::WriteHalf) {
        let (read, write) = self.stream.split();
        let session = Rc::new(RefCell::new(self.session));
        (
            TlsReadHalf {
                session: Rc::clone(&session),
                stream: read,
                incoming: self.incoming,
                plaintext: self.plaintext,
            },
            TlsWriteHalf {
                session,
                stream: write,
                outgoing: self.outgoing,
            },
        )
    }
}

impl<R: AsyncRead> AsyncRead for TlsReadHalf<R> {
    /// The peer's plaintext, reading records from the stream until some arrives: 0 once the peer sent its `close_notify`,
    /// `UnexpectedEof` when the stream closed without it.
    async fn read<B: IoBufMut>(&mut self, mut buf: B) -> BufResult<usize, B> {
        let room = buf.buf_capacity();
        if room == 0 {
            return BufResult(Ok(0), buf);
        }
        for _ in 0..READ_TURNS_MAX {
            let want = room.min(self.plaintext.len());
            let into = self.plaintext.get_mut(..want).unwrap_or_default();
            let taken = self.session.borrow_mut().read(into);
            match taken {
                Plaintext::Read(n) => {
                    let mut source: &[u8] = self.plaintext.get(..n).unwrap_or_default();
                    return source.read(buf).await;
                }
                Plaintext::Closed => return BufResult(Ok(0), buf),
                Plaintext::Truncated => {
                    return BufResult(Err(io::ErrorKind::UnexpectedEof.into()), buf);
                }
                Plaintext::Pending => {}
            }
            let mut incoming = std::mem::take(&mut self.incoming);
            incoming.clear();
            incoming.reserve_exact(RECORD_SIZE_MAX);
            let BufResult(read, incoming) = self.stream.read(incoming).await;
            self.incoming = incoming;
            if let Err(error) = read {
                return BufResult(Err(error), buf);
            }
            if let Err(refusal) = self.session.borrow_mut().receive(&self.incoming) {
                return BufResult(Err(io::Error::other(refusal.to_string())), buf);
            }
        }
        let exhausted = io::Error::other(format!("no plaintext after {READ_TURNS_MAX} reads"));
        BufResult(Err(exhausted), buf)
    }
}

impl<W: AsyncWrite> TlsWriteHalf<W> {
    /// Writes every record the session has to the stream, and flushes it. No borrow of the session is held across a wait.
    async fn write_records(&mut self) -> io::Result<()> {
        // Ends: each turn writes at least one byte the session queued, and a turn adds no record of its own.
        while self.session.borrow().wants_write() {
            let mut outgoing = std::mem::take(&mut self.outgoing);
            outgoing.resize(RECORD_SIZE_MAX, 0);
            let n = self.session.borrow_mut().transmit(&mut outgoing);
            let BufResult(written, records) = self.stream.write_all(outgoing.slice(..n)).await;
            self.outgoing = records.into_inner();
            written?;
        }
        self.stream.flush().await
    }
}

impl<W: AsyncWrite> AsyncWrite for TlsWriteHalf<W> {
    /// Sends `buf` whole as records, written to the stream before this returns.
    async fn write<T: IoBuf>(&mut self, buf: T) -> BufResult<usize, T> {
        let len = buf.as_init().len();
        let sent = self.session.borrow_mut().send(buf.as_init());
        if let Err(refusal) = sent {
            return BufResult(Err(io::Error::other(refusal.to_string())), buf);
        }
        match self.write_records().await {
            Ok(()) => BufResult(Ok(len), buf),
            Err(error) => BufResult(Err(error), buf),
        }
    }

    /// Writes every record waiting and flushes the stream.
    async fn flush(&mut self) -> io::Result<()> {
        self.write_records().await
    }

    /// Sends the session's `close_notify` and shuts the stream's sending side.
    async fn shutdown(&mut self) -> io::Result<()> {
        self.session.borrow_mut().close();
        self.write_records().await?;
        self.stream.shutdown().await
    }
}
