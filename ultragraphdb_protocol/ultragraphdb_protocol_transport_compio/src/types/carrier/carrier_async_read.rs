/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::BufResult;
use compio::buf::IoBufMut;
use compio::io::AsyncRead;

use super::Carrier;

impl AsyncRead for Carrier {
    /// Reads from the socket into `buf`. A future dropped at its read loses the bytes it read, if any, with `buf`.
    async fn read<B: IoBufMut>(&mut self, buf: B) -> BufResult<usize, B> {
        match self {
            Self::Tcp(stream) => stream.read(buf).await,
            Self::Unix(stream) => stream.read(buf).await,
        }
    }
}
