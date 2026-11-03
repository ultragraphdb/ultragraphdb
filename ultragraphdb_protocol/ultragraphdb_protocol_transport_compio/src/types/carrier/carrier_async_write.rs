/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::BufResult;
use compio::buf::IoBuf;
use compio::io::AsyncWrite;

use super::Carrier;

impl AsyncWrite for Carrier {
    async fn write<T: IoBuf>(&mut self, buf: T) -> BufResult<usize, T> {
        match self {
            Self::Tcp(stream) => stream.write(buf).await,
            Self::Unix(stream) => stream.write(buf).await,
        }
    }

    async fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Tcp(stream) => stream.flush().await,
            Self::Unix(stream) => stream.flush().await,
        }
    }

    /// Shuts down the sending side: the peer reads zero bytes once it has read what was sent before.
    async fn shutdown(&mut self) -> std::io::Result<()> {
        match self {
            Self::Tcp(stream) => stream.shutdown().await,
            Self::Unix(stream) => stream.shutdown().await,
        }
    }
}
