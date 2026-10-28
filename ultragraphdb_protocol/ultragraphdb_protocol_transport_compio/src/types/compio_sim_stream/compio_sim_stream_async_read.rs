/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::future::poll_fn;

use compio::BufResult;
use compio::buf::IoBufMut;
use compio::io::AsyncRead;

use crate::types::compio_sim_stream::CompioSimStream;

impl AsyncRead for CompioSimStream {
    async fn read<B: IoBufMut>(&mut self, mut buf: B) -> BufResult<usize, B> {
        let room = buf.buf_capacity().saturating_sub(buf.buf_len());
        match poll_fn(|cx| self.stream.poll_chunk(cx, room)).await {
            None => BufResult(Ok(0), buf),
            // compio's own reader of a byte slice copies into the buffer; the chunk was cut to the buffer's room.
            Some(chunk) => {
                let mut source: &[u8] = &chunk;
                source.read(buf).await
            }
        }
    }
}
