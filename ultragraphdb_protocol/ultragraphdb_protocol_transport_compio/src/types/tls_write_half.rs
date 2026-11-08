/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;
use std::io;
use std::rc::Rc;

use compio::BufResult;
use compio::buf::{IntoInner, IoBuf};
use compio::io::{AsyncWrite, AsyncWriteExt};
use ultragraphdb_transport_tls::{RECORD_SIZE_MAX, TlsSession};

/// A TLS connection's sending half, whose session its receiving half shares on the same core.
#[derive(Debug)]
pub struct TlsWriteHalf<W> {
    session: Rc<RefCell<TlsSession>>,
    stream: W,
    outgoing: Vec<u8>,
}

impl<W> TlsWriteHalf<W> {
    /// The sending half over `stream` of `session`, with the buffer of the connection it is split from.
    pub(crate) fn new(session: Rc<RefCell<TlsSession>>, stream: W, outgoing: Vec<u8>) -> Self {
        Self {
            session,
            stream,
            outgoing,
        }
    }
}

impl<W: AsyncWrite> TlsWriteHalf<W> {
    /// Writes every record the session has to the stream, and flushes it. No borrow of the session is held across a
    /// wait. A future dropped at a write leaves the records not yet taken queued in the session, and loses those taken
    /// for that write with `outgoing`, written in whole, in part or not at all, which fails the peer's next record; the
    /// next write allocates `outgoing` again. One dropped at the flush has written every record.
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
    /// Sends `buf` whole as records, written to the stream before this returns. The session takes `buf` before the
    /// first `.await`, so a future dropped at a write has queued it as records, which leave as
    /// `TlsWriteHalf::write_records` says.
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

    /// Writes every record waiting and flushes the stream; a future dropped at a wait leaves what
    /// `TlsWriteHalf::write_records` says.
    async fn flush(&mut self) -> io::Result<()> {
        self.write_records().await
    }

    /// Sends the session's `close_notify` and shuts the stream's sending side. A future dropped at a write leaves the
    /// session closed and its records as `TlsWriteHalf::write_records` says; one dropped at the shutdown has sent the
    /// `close_notify`, and the sending side may be open still.
    async fn shutdown(&mut self) -> io::Result<()> {
        self.session.borrow_mut().close();
        self.write_records().await?;
        self.stream.shutdown().await
    }
}
