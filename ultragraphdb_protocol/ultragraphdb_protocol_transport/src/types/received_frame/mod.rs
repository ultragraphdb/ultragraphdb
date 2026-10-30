/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{Command, FrameView, PROTOCOL_VERSION};

use crate::types::pooled_buffer::PooledBuffer;

/// A frame read whole into a pooled buffer, its header verified for the connection's cluster and largest frame; its
/// chunks, its sequence and protocol, and its body are checked when it is viewed.
#[derive(Debug)]
pub struct ReceivedFrame {
    buffer: PooledBuffer,
    cluster: u64,
    max_frame: usize,
    client: bool,
    sequence: u128,
    protocol: u16,
}

impl ReceivedFrame {
    /// The frame in `buffer` of a connection of `cluster` taking frames of at most `max_frame` bytes, sent by a client
    /// when `client`, expected at `sequence` and, but for Hello and Welcome, at `protocol`.
    pub(crate) fn new(
        buffer: PooledBuffer,
        cluster: u64,
        max_frame: usize,
        client: bool,
        sequence: u128,
        protocol: u16,
    ) -> Self {
        Self {
            buffer,
            cluster,
            max_frame,
            client,
            sequence,
            protocol,
        }
    }

    /// The frame's bytes, header and body.
    pub fn bytes(&self) -> &[u8] {
        self.buffer.as_slice()
    }

    /// The frame validated in place for the connection, in `PROTOCOL.md` §2.5's order: the header and chunks
    /// ([`FrameView::validate`], or [`FrameView::validate_client`] for a client's frame), then its sequence, the one
    /// the connection expected ([`ProtocolError::SequenceOrder`]), then its protocol, 1 in Hello and Welcome and the
    /// connection's after them ([`ProtocolError::FieldValue`] `"protocol"`).
    pub fn view(&self) -> Result<FrameView<'_>, ProtocolError> {
        let bytes = self.buffer.as_slice();
        let view = if self.client {
            FrameView::validate_client(bytes, self.cluster, self.max_frame)
        } else {
            FrameView::validate(bytes, self.cluster, self.max_frame)
        }?;
        let header = view.header();
        if header.sequence() != self.sequence {
            return Err(ProtocolError::SequenceOrder(
                self.sequence,
                header.sequence(),
            ));
        }
        let protocol = match view.command() {
            Command::Hello | Command::Welcome => PROTOCOL_VERSION,
            _ => self.protocol,
        };
        if header.protocol() != protocol {
            return Err(ProtocolError::FieldValue(
                header.command(),
                "protocol",
                header.protocol().into(),
            ));
        }
        Ok(view)
    }

    /// The sequence the connection expected this frame at.
    pub fn expected_sequence(&self) -> u128 {
        self.sequence
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
