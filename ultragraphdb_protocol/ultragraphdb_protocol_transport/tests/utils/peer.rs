/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A client's end for the server connection's tests: each frame numbered as a client sends it, then read by a
//! server's framer, as a shell reads one.

use std::time::Instant;

use ultragraphdb_protocol_transport::{BufferPool, Framer, ReceivedFrame};
use ultragraphdb_protocol_wire::{BodyBuilder, Command, Header, MAX_FRAME_SIZE};

use crate::utils::feed::feed;

pub const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

pub struct Peer {
    framer: Framer,
    pool: BufferPool,
    sequence: u128,
}

impl Default for Peer {
    fn default() -> Self {
        Self {
            framer: Framer::from_client(CLUSTER, MAX_FRAME_SIZE),
            pool: BufferPool::new(64, MAX_FRAME_SIZE),
            sequence: 0,
        }
    }
}

impl Peer {
    /// `body` on `stream`, the client's next frame, as the server reads it.
    pub fn send<B: BodyBuilder>(&mut self, stream: u64, body: &B) -> ReceivedFrame {
        self.sequence += 1;
        let header = Header::new(CLUSTER, Command::Abort)
            .with_sequence(self.sequence)
            .with_stream(stream);
        self.raw(body.frame(header).unwrap())
    }

    /// `frame`, as built, as the server reads it.
    pub fn raw(&mut self, frame: Vec<u8>) -> ReceivedFrame {
        feed(&mut self.framer, &self.pool, &frame, &mut 0, 1 << 20).unwrap()
    }

    /// The sequence the client's next frame takes.
    pub fn skip(&mut self) {
        self.sequence += 1;
    }

    pub fn framer_mut(&mut self) -> &mut Framer {
        &mut self.framer
    }
}

/// A point in time to start from. The connection never reads a clock; the tests move this value on themselves, so the
/// clock is read once, here, for an origin only.
#[allow(clippy::disallowed_methods)]
pub fn start() -> Instant {
    Instant::now()
}
