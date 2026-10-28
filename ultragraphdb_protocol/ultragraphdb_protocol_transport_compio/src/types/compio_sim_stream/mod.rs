/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod compio_sim_stream_async_read;
mod compio_sim_stream_async_write;

use ultragraphdb_protocol_transport::SimStream;

/// A [`SimStream`] as a compio stream: a write sends one message, a read takes what the model hands over.
#[derive(Debug, Clone)]
pub struct CompioSimStream {
    stream: SimStream,
}

impl CompioSimStream {
    pub fn new(stream: SimStream) -> Self {
        Self { stream }
    }

    /// The simulated stream, for its own operations.
    pub fn sim(&self) -> &SimStream {
        &self.stream
    }
}
