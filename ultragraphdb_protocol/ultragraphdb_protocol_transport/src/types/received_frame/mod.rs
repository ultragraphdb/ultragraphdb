/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{FrameView, MAX_FRAME_SIZE};

use crate::types::pooled_buffer::PooledBuffer;

/// A frame read whole into a pooled buffer, its header verified for the connection's cluster; its chunks and body are
/// checked when it is viewed.
#[derive(Debug)]
pub struct ReceivedFrame {
    buffer: PooledBuffer,
    cluster: u64,
}

impl ReceivedFrame {
    pub(crate) fn new(buffer: PooledBuffer, cluster: u64) -> Self {
        Self { buffer, cluster }
    }

    /// The frame's bytes, header and body.
    pub fn bytes(&self) -> &[u8] {
        self.buffer.as_slice()
    }

    /// The frame validated in place ([`FrameView::validate`]) for the connection's cluster.
    pub fn view(&self) -> Result<FrameView<'_>, ProtocolError> {
        FrameView::validate(self.buffer.as_slice(), self.cluster, MAX_FRAME_SIZE)
    }

    /// The header's `command` number.
    pub fn command(&self) -> u16 {
        self.field::<2>(54).map_or(0, u16::from_le_bytes)
    }

    /// The header's `stream`.
    pub fn stream(&self) -> u64 {
        self.field::<8>(56).map_or(0, u64::from_le_bytes)
    }

    /// The `N` header bytes at `at`; present in every frame read whole.
    fn field<const N: usize>(&self, at: usize) -> Option<[u8; N]> {
        self.buffer.as_slice().get(at..at + N)?.try_into().ok()
    }

    /// The header's `flags`.
    pub fn flags(&self) -> u8 {
        self.buffer.as_slice().get(65).copied().unwrap_or(0)
    }
}
