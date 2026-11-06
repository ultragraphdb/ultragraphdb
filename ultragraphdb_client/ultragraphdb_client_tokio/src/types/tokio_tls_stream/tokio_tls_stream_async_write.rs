/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use tokio::io::{AsyncRead, AsyncWrite};

use super::TokioTlsStream;

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncWrite for TokioTlsStream<S> {
    /// Sends `buf` whole as records once those of the last write have gone out, and starts them on their way; a flush
    /// sees them out.
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        ready!(this.poll_write_records(cx))?;
        if let Err(e) = this.session_mut().send(buf) {
            return Poll::Ready(Err(io::Error::other(e.to_string())));
        }
        if let Poll::Ready(Err(e)) = this.poll_write_records(cx) {
            return Poll::Ready(Err(e));
        }
        Poll::Ready(Ok(buf.len()))
    }

    /// Writes every record waiting and flushes the stream.
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        ready!(this.poll_write_records(cx))?;
        Pin::new(this.stream_mut()).poll_flush(cx)
    }

    /// Sends the session's `close_notify` and shuts the stream's sending side.
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        this.session_mut().close();
        ready!(this.poll_write_records(cx))?;
        Pin::new(this.stream_mut()).poll_shutdown(cx)
    }
}
