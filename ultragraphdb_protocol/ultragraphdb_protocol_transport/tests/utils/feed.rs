/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A synchronous shell for the tests: the bytes of a peer's stream fed to a [`Framer`] in reads of at most `step`
//! bytes, each landing in the frame's pooled buffer as the framer asks.

use ultragraphdb_protocol_transport::{
    BufferPool, FrameStep, Framer, ReceivedFrame, TransportError,
};

/// The next frame of `stream`, read from `*at` in reads of at most `step` bytes; `*at` moves past what was read.
pub fn feed(
    framer: &mut Framer,
    pool: &BufferPool,
    stream: &[u8],
    at: &mut usize,
    step: usize,
) -> Result<ReceivedFrame, TransportError> {
    let mut buffer = pool.acquire()?;
    loop {
        let mut bytes = buffer.take();
        while bytes.len() < framer.want() {
            let room = (framer.want() - bytes.len()).min(step.max(1));
            let Some(read) = stream.get(*at..(*at + room).min(stream.len())) else {
                break;
            };
            if read.is_empty() {
                break;
            }
            bytes.extend_from_slice(read);
            *at += read.len();
        }
        let received = bytes.len();
        let short = received < framer.want();
        buffer.restore(bytes);
        if short {
            return Err(framer.closed(received));
        }
        match framer.advance(&buffer)? {
            FrameStep::Read(_) => {}
            FrameStep::Frame => return Ok(framer.finish(buffer)),
        }
    }
}
