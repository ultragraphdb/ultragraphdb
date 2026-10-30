/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod header_encode;

use crate::types::command::Command;
use crate::utils::constants::{COMMAND_HEADER_SIZE, PROTOCOL_VERSION};

/// The header's fields a sender chooses, to be written in front of a body. `size`, `checksum_body` and `checksum`
/// follow from the body and the other fields, so [`Header::encode_frame`] fills them in, and the reserved bytes are
/// always zero.
///
/// | Offset | Field | Type |
/// |---|---|---|
/// | 0 | `checksum` | u64, XXH3-64 of bytes 8 to 256 |
/// | 8 | `checksum_body` | u64 |
/// | 16 | `sequence` | u128, the frame's place among its sender's frames on the connection, from 1 |
/// | 32 | `cluster` | u64 |
/// | 40 | `size` | u32, header and body |
/// | 44 | `view` | u32 |
/// | 48 | `release` | u32 |
/// | 52 | `protocol` | u16 |
/// | 54 | `command` | u16 |
/// | 56 | `stream` | u64 |
/// | 64 | `replica` | u8 |
/// | 65 | `flags` | u8 |
/// | 66 | reserved | 62 bytes, zero |
/// | 128 | `command_header` | 128 bytes |
///
/// Every field is little-endian.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Header {
    sequence: u128,
    cluster: u64,
    view: u32,
    release: u32,
    protocol: u16,
    command: Command,
    stream: u64,
    replica: u8,
    flags: u8,
    command_header: [u8; COMMAND_HEADER_SIZE],
}

impl Header {
    /// A header of `command` for `cluster`, at [`crate::PROTOCOL_VERSION`], with every other field zero: sequence,
    /// view, release, stream, replica, flags and the command header.
    pub fn new(cluster: u64, command: Command) -> Self {
        Self {
            sequence: 0,
            cluster,
            view: 0,
            release: 0,
            protocol: PROTOCOL_VERSION,
            command,
            stream: 0,
            replica: 0,
            flags: 0,
            command_header: [0; COMMAND_HEADER_SIZE],
        }
    }

    /// The header of `command`.
    pub fn with_command(mut self, command: Command) -> Self {
        self.command = command;
        self
    }

    /// The header with its `sequence`: its sender's count of the frames it sent on the connection, this one
    /// included; a log frame's round position.
    pub fn with_sequence(mut self, sequence: u128) -> Self {
        self.sequence = sequence;
        self
    }

    /// The header with its VSR `view`; zero on one node and in client frames.
    pub fn with_view(mut self, view: u32) -> Self {
        self.view = view;
        self
    }

    /// The header with the sender's `release`.
    pub fn with_release(mut self, release: u32) -> Self {
        self.release = release;
        self
    }

    /// The header with the frame format's version `protocol`.
    pub fn with_protocol(mut self, protocol: u16) -> Self {
        self.protocol = protocol;
        self
    }

    /// The header on `stream`, the request the frame belongs to; zero for connection-level frames.
    pub fn with_stream(mut self, stream: u64) -> Self {
        self.stream = stream;
        self
    }

    /// The header from `replica`, the sending replica; zero from clients.
    pub fn with_replica(mut self, replica: u8) -> Self {
        self.replica = replica;
        self
    }

    /// The header with `flags`.
    pub fn with_flags(mut self, flags: u8) -> Self {
        self.flags = flags;
        self
    }

    /// The header with the command's own 128-byte header.
    pub fn with_command_header(mut self, command_header: [u8; COMMAND_HEADER_SIZE]) -> Self {
        self.command_header = command_header;
        self
    }

    pub fn sequence(&self) -> u128 {
        self.sequence
    }

    pub fn cluster(&self) -> u64 {
        self.cluster
    }

    pub fn view(&self) -> u32 {
        self.view
    }

    pub fn release(&self) -> u32 {
        self.release
    }

    pub fn protocol(&self) -> u16 {
        self.protocol
    }

    pub fn command(&self) -> Command {
        self.command
    }

    pub fn stream(&self) -> u64 {
        self.stream
    }

    pub fn replica(&self) -> u8 {
        self.replica
    }

    pub fn flags(&self) -> u8 {
        self.flags
    }

    pub fn command_header(&self) -> &[u8; COMMAND_HEADER_SIZE] {
        &self.command_header
    }
}
