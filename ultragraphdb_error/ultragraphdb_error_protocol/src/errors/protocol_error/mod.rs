/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod protocol_error_display;
mod protocol_error_error;

/// Why a frame was refused: one variant per validation rule, raised before the command's own rules run: the header's
/// and the chunks' rules, then the body's structure (its sections, records, counts, text and terms). A receiver
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
    /// A view of command `expected` was asked of a frame of command `found`.
    WrongCommand { expected: u16, found: u16 },
    /// A body of command `command`, `len` bytes, is shorter than its fixed part and section table, `need` bytes.
    BodyTooShort {
        command: u16,
        len: usize,
        need: usize,
    },
    /// Section `section` of a body of command `command`, `len` bytes at `offset`, runs past the body's `body` bytes.
    SectionOutOfBounds {
        command: u16,
        section: usize,
        offset: u32,
        len: u32,
        body: usize,
    },
    /// Section `section` starts at `offset`, not at `expected`: the end of the part before it rounded up to 8 bytes.
    /// An overlapping, out-of-order, misaligned or gapped section.
    SectionOffset {
        command: u16,
        section: usize,
        offset: u32,
        expected: usize,
    },
    /// The padding byte at body offset `offset` is not zero.
    PaddingNotZero { command: u16, offset: usize },
    /// The body holds `len` bytes, past the end of its last section at `end`.
    TrailingBytes {
        command: u16,
        end: usize,
        len: usize,
    },
    /// Section `section`, `len` bytes, is not a whole number of `unit`-byte records.
    SectionLength {
        command: u16,
        section: usize,
        len: usize,
        unit: usize,
    },
    /// Entry `entry` of the key-value section `section`, at section offset `offset`, runs past the section.
    EntryOutOfBounds {
        command: u16,
        section: usize,
        entry: usize,
        offset: usize,
    },
    /// `field` counts `count` where the body holds `expected`.
    CountMismatch {
        command: u16,
        field: &'static str,
        count: u64,
        expected: u64,
    },
    /// The text of section `section` is not UTF-8 at section offset `offset`.
    NotUtf8 {
        command: u16,
        section: usize,
        offset: usize,
    },
    /// Entry `at` of section `section` is `index`, past the `limit` entries it indexes.
    IndexOutOfRange {
        command: u16,
        section: usize,
        at: usize,
        index: u32,
        limit: u32,
    },
    /// Term `at` of section `section` is not a term: not `ultragraphdb_core_terms`' encoding, or not an RDF term of
    /// the kind its place takes in N-Triples syntax.
    BadTerm {
        command: u16,
        section: usize,
        at: usize,
    },
    /// `field` holds `value`, which the command does not define.
    FieldValue {
        command: u16,
        field: &'static str,
        value: u64,
    },
    /// Write-set record `index` of a Round body is refused for `cause`.
    Record {
        index: usize,
        cause: Box<ProtocolErrorEnum>,
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

    #[allow(non_snake_case)]
    pub fn WrongCommand(expected: u16, found: u16) -> Self {
        Self(ProtocolErrorEnum::WrongCommand { expected, found })
    }

    #[allow(non_snake_case)]
    pub fn BodyTooShort(command: u16, len: usize, need: usize) -> Self {
        Self(ProtocolErrorEnum::BodyTooShort { command, len, need })
    }

    #[allow(non_snake_case)]
    pub fn SectionOutOfBounds(
        command: u16,
        section: usize,
        offset: u32,
        len: u32,
        body: usize,
    ) -> Self {
        Self(ProtocolErrorEnum::SectionOutOfBounds {
            command,
            section,
            offset,
            len,
            body,
        })
    }

    #[allow(non_snake_case)]
    pub fn SectionOffset(command: u16, section: usize, offset: u32, expected: usize) -> Self {
        Self(ProtocolErrorEnum::SectionOffset {
            command,
            section,
            offset,
            expected,
        })
    }

    #[allow(non_snake_case)]
    pub fn PaddingNotZero(command: u16, offset: usize) -> Self {
        Self(ProtocolErrorEnum::PaddingNotZero { command, offset })
    }

    #[allow(non_snake_case)]
    pub fn TrailingBytes(command: u16, end: usize, len: usize) -> Self {
        Self(ProtocolErrorEnum::TrailingBytes { command, end, len })
    }

    #[allow(non_snake_case)]
    pub fn SectionLength(command: u16, section: usize, len: usize, unit: usize) -> Self {
        Self(ProtocolErrorEnum::SectionLength {
            command,
            section,
            len,
            unit,
        })
    }

    #[allow(non_snake_case)]
    pub fn EntryOutOfBounds(command: u16, section: usize, entry: usize, offset: usize) -> Self {
        Self(ProtocolErrorEnum::EntryOutOfBounds {
            command,
            section,
            entry,
            offset,
        })
    }

    #[allow(non_snake_case)]
    pub fn CountMismatch(command: u16, field: &'static str, count: u64, expected: u64) -> Self {
        Self(ProtocolErrorEnum::CountMismatch {
            command,
            field,
            count,
            expected,
        })
    }

    #[allow(non_snake_case)]
    pub fn NotUtf8(command: u16, section: usize, offset: usize) -> Self {
        Self(ProtocolErrorEnum::NotUtf8 {
            command,
            section,
            offset,
        })
    }

    #[allow(non_snake_case)]
    pub fn IndexOutOfRange(
        command: u16,
        section: usize,
        at: usize,
        index: u32,
        limit: u32,
    ) -> Self {
        Self(ProtocolErrorEnum::IndexOutOfRange {
            command,
            section,
            at,
            index,
            limit,
        })
    }

    #[allow(non_snake_case)]
    pub fn BadTerm(command: u16, section: usize, at: usize) -> Self {
        Self(ProtocolErrorEnum::BadTerm {
            command,
            section,
            at,
        })
    }

    #[allow(non_snake_case)]
    pub fn FieldValue(command: u16, field: &'static str, value: u64) -> Self {
        Self(ProtocolErrorEnum::FieldValue {
            command,
            field,
            value,
        })
    }

    /// Record `index`'s refusal `cause`, as a Round body reports it.
    #[allow(non_snake_case)]
    pub fn Record(index: usize, cause: ProtocolError) -> Self {
        Self(ProtocolErrorEnum::Record {
            index,
            cause: Box::new(cause.0),
        })
    }
}
