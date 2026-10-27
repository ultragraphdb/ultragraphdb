/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::future::poll_fn;
use std::task::Poll;

use compio::BufResult;
use compio::buf::IoBufMut;
use compio::io::AsyncRead;

use crate::types::sim_stream::SimStream;

impl AsyncRead for SimStream {
    async fn read<B: IoBufMut>(&mut self, mut buf: B) -> BufResult<usize, B> {
        let room = buf.buf_capacity().saturating_sub(buf.buf_len());
        let outcome = poll_fn(|cx| {
            let mut state = self.state.borrow_mut();
            loop {
                let arrived = state
                    .links
                    .get(self.incoming)
                    .is_some_and(|link| !link.arrived.is_empty());
                if arrived || room == 0 {
                    break;
                }
                if state
                    .links
                    .get(self.incoming)
                    .is_some_and(|link| !link.in_flight.is_empty())
                {
                    state.step();
                    continue;
                }
                let Some(link) = state.links.get_mut(self.incoming) else {
                    return Poll::Ready(None);
                };
                if link.closed {
                    return Poll::Ready(None);
                }
                link.reader = Some(cx.waker().clone());
                return Poll::Pending;
            }
            let Some(link) = state.links.get_mut(self.incoming) else {
                return Poll::Ready(None);
            };
            let Some(mut chunk) = link.arrived.pop_front() else {
                return Poll::Ready(None);
            };
            if chunk.len() > room {
                let rest = chunk.split_off(room);
                link.arrived.push_front(rest);
            }
            Poll::Ready(Some(chunk))
        })
        .await;
        match outcome {
            None => BufResult(Ok(0), buf),
            // compio's own reader of a byte slice copies into the buffer; the chunk was cut to the buffer's room.
            Some(chunk) => {
                let mut source: &[u8] = &chunk;
                source.read(buf).await
            }
        }
    }
}
