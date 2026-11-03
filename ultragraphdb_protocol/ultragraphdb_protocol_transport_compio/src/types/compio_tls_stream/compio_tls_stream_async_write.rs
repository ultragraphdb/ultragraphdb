/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::BufResult;
use compio::buf::IoBuf;
use compio::io::{AsyncRead, AsyncWrite};

use super::CompioTlsStream;

impl<S: AsyncRead + AsyncWrite> AsyncWrite for CompioTlsStream<S> {
    /// Sends `buf` whole as records, written to the stream before this returns.
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

    /// Writes every record waiting and flushes the stream.
    async fn flush(&mut self) -> std::io::Result<()> {
        self.write_records().await
    }

    /// Sends the session's `close_notify` and shuts the stream's sending side.
    async fn shutdown(&mut self) -> std::io::Result<()> {
        self.session_mut().close();
        self.write_records().await?;
        self.stream_mut().shutdown().await
    }
}
