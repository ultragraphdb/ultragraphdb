/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;
use std::io;
use std::rc::Rc;

use compio::BufResult;
use compio::buf::IoBufMut;
use compio::io::AsyncRead;
use ultragraphdb_transport_tls::{Plaintext, READ_TURNS_MAX, RECORD_SIZE_MAX, TlsSession};

/// A TLS connection's receiving half: a read pending on its stream holds no borrow of the session, so the sending half
/// goes on. What the session comes to owe the peer while reading, an answer to a key update, leaves with the next write.
#[derive(Debug)]
pub struct TlsReadHalf<R> {
    session: Rc<RefCell<TlsSession>>,
    stream: R,
    incoming: Vec<u8>,
    plaintext: Vec<u8>,
}

impl<R> TlsReadHalf<R> {
    /// The receiving half over `stream` of `session`, with the buffers of the connection it is split from.
    pub(crate) fn new(
        session: Rc<RefCell<TlsSession>>,
        stream: R,
        incoming: Vec<u8>,
        plaintext: Vec<u8>,
    ) -> Self {
        Self {
            session,
            stream,
            incoming,
            plaintext,
        }
    }
}

impl<R: AsyncRead> AsyncRead for TlsReadHalf<R> {
    /// The peer's plaintext, reading records from the stream until some arrives: 0 once the peer sent its `close_notify`,
    /// `UnexpectedEof` when the stream closed without it. A future dropped at its read gives the session nothing and
    /// loses `incoming` with the bytes read into it, which fails the session's next record, and the next read allocates
    /// `incoming` again; plaintext is taken from the session only when it is copied into `buf` in the same poll.
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
