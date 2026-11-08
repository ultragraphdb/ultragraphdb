/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::BufResult;
use compio::buf::IoBuf;
use compio::io::{AsyncRead, AsyncWrite};

use super::CompioTlsStream;

impl<S: AsyncRead + AsyncWrite> AsyncWrite for CompioTlsStream<S> {
    /// Sends `buf` whole as records, written to the stream before this returns. The session takes `buf` before the
    /// first `.await`, so a future dropped at a write has queued it as records, which leave as
    /// `CompioTlsStream::write_records` says.
    async fn write<T: IoBuf>(&mut self, buf: T) -> BufResult<usize, T> {
        let len = buf.as_init().len();
        if let Err(e) = self.session_mut().send(buf.as_init()) {
            return BufResult(Err(std::io::Error::other(e.to_string())), buf);
        }
        match self.write_records().await {
            Ok(()) => BufResult(Ok(len), buf),
            Err(e) => BufResult(Err(e), buf),
        }
    }

    /// Writes every record waiting and flushes the stream; a future dropped at a wait leaves what
    /// `CompioTlsStream::write_records` says.
    async fn flush(&mut self) -> std::io::Result<()> {
        self.write_records().await
    }

    /// Sends the session's `close_notify` and shuts the stream's sending side. A future dropped at a write leaves the
    /// session closed and its records as `CompioTlsStream::write_records` says; one dropped at the shutdown has sent
    /// the `close_notify`, and the sending side may be open still.
    async fn shutdown(&mut self) -> std::io::Result<()> {
        self.session_mut().close();
        self.write_records().await?;
        self.stream_mut().shutdown().await
    }
}
