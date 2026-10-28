/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod tokio_sim_stream_async_read;
mod tokio_sim_stream_async_write;

use ultragraphdb_protocol_transport::SimStream;

/// A [`SimStream`] as a tokio stream: a write sends one message, a read takes what the model hands over.
#[derive(Debug, Clone)]
pub struct TokioSimStream {
    stream: SimStream,
}

impl TokioSimStream {
    pub fn new(stream: SimStream) -> Self {
        Self { stream }
    }

    pub fn sim(&self) -> &SimStream {
        &self.stream
    }
}
