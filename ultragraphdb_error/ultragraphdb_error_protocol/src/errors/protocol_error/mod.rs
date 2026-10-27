/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod protocol_error_display;
mod protocol_error_error;

/// Why a frame was refused: one variant per validation rule, raised before the command's own rules run. A receiver
/// answers it with an error frame where the connection can still carry one, then closes the connection.
///
/// The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProtocolErrorEnum {
    /// The frame holds `len` bytes, fewer than the 256 of a header. The length rule, checked first.
    ShortHeader { len: usize },
    /// The header's `checksum` is not XXH3-64 of its bytes 8 to 256: `stored` is the field, `computed` the hash.
    HeaderChecksum { stored: u64, computed: u64 },
    /// The `command` number names no command of this protocol: never assigned, retired, or reserved for the replica
    /// commands of a later stage.
    UnknownCommand { command: u16 },
    /// A reserved header byte is not zero: `offset` is the first such byte, in `nonce_reserved` (16 to 32) or in the
    /// reserved bytes (66 to 128).
    ReservedNotZero { offset: usize },
    /// The frame's `cluster` is `found`, not the receiver's `expected`.
    WrongCluster { expected: u64, found: u64 },
    /// The header's `size` is below the 256 bytes of the header it is part of.
    SizeBelowHeader { size: u32 },
    /// A frame of `size` bytes, header and body, is larger than `limit`, the 1 MiB maximum: stated by a header
    /// received, or what a frame being built would take.
    FrameTooLarge { size: usize, limit: usize },
    /// The frame's `size` is not the `len` bytes received: a truncated body, or bytes past the frame.
    LengthMismatch { size: u32, len: usize },
    /// A body of `len` bytes has no split into chunks and their table.
    BodyLength { len: usize },
    /// The chunk table's XXH3-64, `computed`, is not the header's `checksum_body`, `stored`: the table broke, and no
    /// chunk is trusted.
    ChunkTable { stored: u64, computed: u64 },
    /// Chunk `index`, the body's bytes `start` to `end`, does not match its leaf.
    Chunk {
        index: usize,
        start: usize,
        end: usize,
    },
}

/// Why a frame of the binary protocol was refused. Each variant names the validation rule a frame failed and the
/// values it failed on.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProtocolError(pub ProtocolErrorEnum);

impl ProtocolError {
    #[allow(non_snake_case)]
    pub fn ShortHeader(len: usize) -> Self {
        Self(ProtocolErrorEnum::ShortHeader { len })
    }

    #[allow(non_snake_case)]
    pub fn HeaderChecksum(stored: u64, computed: u64) -> Self {
        Self(ProtocolErrorEnum::HeaderChecksum { stored, computed })
    }

    #[allow(non_snake_case)]
    pub fn UnknownCommand(command: u16) -> Self {
        Self(ProtocolErrorEnum::UnknownCommand { command })
    }

    #[allow(non_snake_case)]
    pub fn ReservedNotZero(offset: usize) -> Self {
        Self(ProtocolErrorEnum::ReservedNotZero { offset })
    }

    #[allow(non_snake_case)]
    pub fn WrongCluster(expected: u64, found: u64) -> Self {
        Self(ProtocolErrorEnum::WrongCluster { expected, found })
    }

    #[allow(non_snake_case)]
    pub fn SizeBelowHeader(size: u32) -> Self {
        Self(ProtocolErrorEnum::SizeBelowHeader { size })
    }

    #[allow(non_snake_case)]
    pub fn FrameTooLarge(size: usize, limit: usize) -> Self {
        Self(ProtocolErrorEnum::FrameTooLarge { size, limit })
    }

    #[allow(non_snake_case)]
    pub fn LengthMismatch(size: u32, len: usize) -> Self {
        Self(ProtocolErrorEnum::LengthMismatch { size, len })
    }

    #[allow(non_snake_case)]
    pub fn BodyLength(len: usize) -> Self {
        Self(ProtocolErrorEnum::BodyLength { len })
    }

    #[allow(non_snake_case)]
    pub fn ChunkTable(stored: u64, computed: u64) -> Self {
        Self(ProtocolErrorEnum::ChunkTable { stored, computed })
    }

    #[allow(non_snake_case)]
    pub fn Chunk(index: usize, start: usize, end: usize) -> Self {
        Self(ProtocolErrorEnum::Chunk { index, start, end })
    }
}
