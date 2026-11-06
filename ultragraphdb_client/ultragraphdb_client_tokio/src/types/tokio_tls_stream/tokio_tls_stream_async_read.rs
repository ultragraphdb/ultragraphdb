/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use ultragraphdb_transport_tls::{Plaintext, READ_TURNS_MAX, RECORD_SIZE_MAX};

use super::TokioTlsStream;

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncRead for TokioTlsStream<S> {
    /// The peer's plaintext, reading records from the stream until some arrives: nothing once the peer sent its
    /// `close_notify`, `UnexpectedEof` when the stream closed without it.
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if buf.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        for _ in 0..READ_TURNS_MAX {
            // At most a record's room at a time: a reader's unfilled buffer may be a whole frame's, and this zeroes it.
            let room = buf.remaining().min(RECORD_SIZE_MAX);
            match this.session_mut().read(buf.initialize_unfilled_to(room)) {
                Plaintext::Read(n) => {
                    buf.advance(n);
                    return Poll::Ready(Ok(()));
                }
                Plaintext::Closed => return Poll::Ready(Ok(())),
                Plaintext::Truncated => {
                    return Poll::Ready(Err(io::ErrorKind::UnexpectedEof.into()));
                }
                Plaintext::Pending => {}
            }
            // What the session owes the peer, such as an answer to a key update, goes out before it waits.
            ready!(this.poll_write_records(cx))?;
            match ready!(this.poll_read_records(cx)) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Poll::Ready(Err(e)),
            }
        }
        let exhausted = io::Error::other(format!("no plaintext after {READ_TURNS_MAX} reads"));
        Poll::Ready(Err(exhausted))
    }
}
