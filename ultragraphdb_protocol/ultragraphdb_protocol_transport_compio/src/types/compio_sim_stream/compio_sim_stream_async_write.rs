/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::BufResult;
use compio::buf::IoBuf;
use compio::io::AsyncWrite;

use crate::types::compio_sim_stream::CompioSimStream;

impl AsyncWrite for CompioSimStream {
    async fn write<T: IoBuf>(&mut self, buf: T) -> BufResult<usize, T> {
        let message = buf.as_init().to_vec();
        let len = message.len();
        self.stream.send(message);
        BufResult(Ok(len), buf)
    }

    async fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }

    /// Shuts down this end's side: the peer reads zero bytes once it has read what was sent before.
    async fn shutdown(&mut self) -> std::io::Result<()> {
        self.stream.shutdown();
        Ok(())
    }
}
