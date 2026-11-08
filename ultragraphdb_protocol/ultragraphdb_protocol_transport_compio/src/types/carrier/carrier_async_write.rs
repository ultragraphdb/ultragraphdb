/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::BufResult;
use compio::buf::IoBuf;
use compio::io::AsyncWrite;

use super::Carrier;

impl AsyncWrite for Carrier {
    /// Writes `buf` to the socket. A future dropped at its write leaves `buf` written in whole, in part or not at all.
    async fn write<T: IoBuf>(&mut self, buf: T) -> BufResult<usize, T> {
        match self {
            Self::Tcp(stream) => stream.write(buf).await,
            Self::Unix(stream) => stream.write(buf).await,
        }
    }

    /// Flushes the socket, which holds no buffer of its own; a future dropped at its flush leaves nothing.
    async fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Tcp(stream) => stream.flush().await,
            Self::Unix(stream) => stream.flush().await,
        }
    }

    /// Shuts down the sending side: the peer reads zero bytes once it has read what was sent before. A future dropped
    /// at its shutdown leaves the sending side shut or open.
    async fn shutdown(&mut self) -> std::io::Result<()> {
        match self {
            Self::Tcp(stream) => stream.shutdown().await,
            Self::Unix(stream) => stream.shutdown().await,
        }
    }
}
