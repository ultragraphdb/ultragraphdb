/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod protocol_error_display;
mod protocol_error_error;

/// Why a frame was refused: one variant per validation rule, raised before the command's own rules run: the header's
/// and the chunks' rules, the connection's sequence, then the body's structure (its sections, records, counts, text
/// and terms). A frame-level rule is a connection error, answered on stream 0 before the connection closes; a body's
/// rule is a stream error, answered on the request's stream, and the connection stays (`PROTOCOL.md` §2.6).
///
/// The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProtocolErrorEnum {
    /// The frame holds `len` bytes, fewer than the 256 of a header. The length rule, checked first.
    ShortHeader {
        /// The bytes the frame holds.
        len: usize,
    },
    /// The header's `checksum` is not XXH3-64 of its bytes 8 to 256: `stored` is the field, `computed` the hash.
    HeaderChecksum {
        /// The checksum the frame carries.
        stored: u64,
        /// The checksum computed over the bytes.
        computed: u64,
    },
    /// The `command` number names no command of this protocol: never assigned, retired, or reserved for the replica
    /// commands of a later stage.
    UnknownCommand {
        /// The command number the header names.
        command: u16,
    },
    /// The command is one of this protocol's, but not one the receiving endpoint serves on this connection: a role
    /// command (512 to 767) on a client connection, or a command a role endpoint does not serve (`PROTOCOL.md` §2.5,
    /// §11.12).
    CommandNotServed {
        /// The command number the header names.
        command: u16,
    },
    /// A reserved header byte is not zero: `offset` is the first such byte, in the reserved bytes (66 to 128) or among
    /// the command header's unused bytes (128 to 256).
    ReservedNotZero {
        /// The header offset of the first non-zero reserved byte.
        offset: usize,
    },
    /// The frame's `cluster` is `found`, not the receiver's `expected`.
    WrongCluster {
        /// The receiver's cluster.
        expected: u64,
        /// The cluster the frame names.
        found: u64,
    },
    /// The header's `size` is below the 256 bytes of the header it is part of.
    SizeBelowHeader {
        /// The header's `size` field.
        size: u32,
    },
    /// A frame of `size` bytes, header and body, is larger than `limit`: the 1 MiB maximum, the `max_frame` the
    /// receiver announced, or a stream receiver's buffer; stated by a header received, or what a frame being built
    /// would take.
    FrameTooLarge {
        /// The frame's size in bytes, header and body.
        size: usize,
        /// The largest frame the receiver takes.
        limit: usize,
    },
    /// The frame's `size` is not the `len` bytes received: a truncated body, or bytes past the frame.
    LengthMismatch {
        /// The header's `size` field.
        size: u32,
        /// The bytes received.
        len: usize,
    },
    /// A body of `len` bytes has no split into chunks and their table.
    BodyLength {
        /// The body's length in bytes.
        len: usize,
    },
    /// The chunk table's XXH3-64, `computed`, is not the header's `checksum_body`, `stored`: the table broke, and no
    /// chunk is trusted.
    ChunkTable {
        /// The header's `checksum_body`.
        stored: u64,
        /// The XXH3-64 of the chunk table.
        computed: u64,
    },
    /// Chunk `index`, the body's bytes `start` to `end`, does not match its leaf.
    Chunk {
        /// The chunk's index.
        index: usize,
        /// The chunk's first body byte.
        start: usize,
        /// The body byte after the chunk's last.
        end: usize,
    },
    /// A connection's frame carries `sequence` `found` where `expected`, one above the previous frame's from the same
    /// sender, was due: a lost, repeated or reordered frame.
    SequenceOrder {
        /// The sequence due: one above the previous frame's.
        expected: u128,
        /// The sequence the frame carries.
        found: u128,
    },
    /// A view of command `expected` was asked of a frame of command `found`.
    WrongCommand {
        /// The command the view reads.
        expected: u16,
        /// The command the frame carries.
        found: u16,
    },
    /// A body of command `command`, `len` bytes, is shorter than its fixed part and section table, `need` bytes.
    BodyTooShort {
        /// The command number of the body.
        command: u16,
        /// The body's length in bytes.
        len: usize,
        /// The length of the body's fixed part and section table.
        need: usize,
    },
    /// Section `section` of a body of command `command`, `len` bytes at `offset`, runs past the body's `body` bytes.
    SectionOutOfBounds {
        /// The command number of the body.
        command: u16,
        /// The section's index in the body's section table.
        section: usize,
        /// The section's offset in the body.
        offset: u32,
        /// The section's length in bytes.
        len: u32,
        /// The body's length in bytes.
        body: usize,
    },
    /// Section `section` starts at `offset`, not at `expected`: the end of the part before it rounded up to 8 bytes.
    /// An overlapping, out-of-order, misaligned or gapped section.
    SectionOffset {
        /// The command number of the body.
        command: u16,
        /// The section's index in the body's section table.
        section: usize,
        /// The offset the section table gives.
        offset: u32,
        /// The offset due: the end of the part before, rounded up to 8 bytes.
        expected: usize,
    },
    /// The padding byte at body offset `offset` is not zero.
    PaddingNotZero {
        /// The command number of the body.
        command: u16,
        /// The body offset of the non-zero padding byte.
        offset: usize,
    },
    /// The body holds `len` bytes, past the end of its last section at `end`.
    TrailingBytes {
        /// The command number of the body.
        command: u16,
        /// The end of the body's last section.
        end: usize,
        /// The body's length in bytes.
        len: usize,
    },
    /// Section `section`, `len` bytes, is not a whole number of `unit`-byte records.
    SectionLength {
        /// The command number of the body.
        command: u16,
        /// The section's index in the body's section table.
        section: usize,
        /// The section's length in bytes.
        len: usize,
        /// The size of one record in bytes.
        unit: usize,
    },
    /// Entry `entry` of the key-value section `section`, at section offset `offset`, runs past the section.
    EntryOutOfBounds {
        /// The command number of the body.
        command: u16,
        /// The section's index in the body's section table.
        section: usize,
        /// The entry's index.
        entry: usize,
        /// The entry's offset within the section.
        offset: usize,
    },
    /// `field` counts `count` where the body holds `expected`.
    CountMismatch {
        /// The command number of the body.
        command: u16,
        /// The field holding the count.
        field: &'static str,
        /// The count the field holds.
        count: u64,
        /// The count the body holds.
        expected: u64,
    },
    /// The text of section `section` is not UTF-8 at section offset `offset`.
    NotUtf8 {
        /// The command number of the body.
        command: u16,
        /// The section's index in the body's section table.
        section: usize,
        /// The section offset of the first byte that is not UTF-8.
        offset: usize,
    },
    /// Entry `at` of section `section` is `index`, past the `limit` entries it indexes.
    IndexOutOfRange {
        /// The command number of the body.
        command: u16,
        /// The section's index in the body's section table.
        section: usize,
        /// The entry's index within the section.
        at: usize,
        /// The index the entry holds.
        index: u32,
        /// The number of entries it indexes.
        limit: u32,
    },
    /// Term `at` of section `section` is not a term: not `ultragraphdb_core_terms`' encoding, or not an RDF term of
    /// the kind its place takes in N-Triples syntax.
    BadTerm {
        /// The command number of the body.
        command: u16,
        /// The section's index in the body's section table.
        section: usize,
        /// The entry's index within the section.
        at: usize,
    },
    /// `field` holds `value`, which the command does not define.
    FieldValue {
        /// The command number of the body.
        command: u16,
        /// The field holding the value.
        field: &'static str,
        /// The value, which the command does not define.
        value: u64,
    },
    /// Entry `at` of the term table in section `section` is the same term as entry `earlier`: a table holds each term
    /// once.
    DuplicateTerm {
        /// The command number of the body.
        command: u16,
        /// The section's index in the body's section table.
        section: usize,
        /// The entry's index within the section.
        at: usize,
        /// The index of the earlier entry holding the same term.
        earlier: usize,
    },
    /// Entry `at` of the term table in section `section` is a triple term nested `depth` deep, past the `limit`.
    TermTooDeep {
        /// The command number of the body.
        command: u16,
        /// The section's index in the body's section table.
        section: usize,
        /// The entry's index within the section.
        at: usize,
        /// The term's nesting depth.
        depth: usize,
        /// The deepest nesting allowed.
        limit: usize,
    },
    /// Write-set record `index` of a Round body is refused for `cause`.
    Record {
        /// The record's index in the Round body.
        index: usize,
        /// Why the record was refused.
        cause: Box<ProtocolErrorEnum>,
    },
    /// A Result or Entries frame arrived on `stream` with the stream's credit spent (`PROTOCOL.md` §11.3).
    PastCredit {
        /// The stream the frame arrived on.
        stream: u64,
    },
}

/// Why a frame of the binary protocol was refused. Each variant names the validation rule a frame failed and the
/// values it failed on.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProtocolError(pub ProtocolErrorEnum);

impl ProtocolError {
    /// A frame of `len` bytes, fewer than a header's 256.
    pub fn short_header(len: usize) -> Self {
        Self(ProtocolErrorEnum::ShortHeader { len })
    }

    /// The header's checksum `stored` is not its XXH3-64, `computed`.
    pub fn header_checksum(stored: u64, computed: u64) -> Self {
        Self(ProtocolErrorEnum::HeaderChecksum { stored, computed })
    }

    /// The `command` number names no command of this protocol.
    pub fn unknown_command(command: u16) -> Self {
        Self(ProtocolErrorEnum::UnknownCommand { command })
    }

    /// The `command` is not one the receiving endpoint serves on this connection.
    pub fn command_not_served(command: u16) -> Self {
        Self(ProtocolErrorEnum::CommandNotServed { command })
    }

    /// The reserved header byte at `offset` is not zero.
    pub fn reserved_not_zero(offset: usize) -> Self {
        Self(ProtocolErrorEnum::ReservedNotZero { offset })
    }

    /// The frame's cluster is `found`, not the receiver's `expected`.
    pub fn wrong_cluster(expected: u64, found: u64) -> Self {
        Self(ProtocolErrorEnum::WrongCluster { expected, found })
    }

    /// The header's `size` is below the 256 bytes of a header.
    pub fn size_below_header(size: u32) -> Self {
        Self(ProtocolErrorEnum::SizeBelowHeader { size })
    }

    /// A frame of `size` bytes is larger than `limit`.
    pub fn frame_too_large(size: usize, limit: usize) -> Self {
        Self(ProtocolErrorEnum::FrameTooLarge { size, limit })
    }

    /// The frame's `size` is not the `len` bytes received.
    pub fn length_mismatch(size: u32, len: usize) -> Self {
        Self(ProtocolErrorEnum::LengthMismatch { size, len })
    }

    /// A body of `len` bytes has no split into chunks and their table.
    pub fn body_length(len: usize) -> Self {
        Self(ProtocolErrorEnum::BodyLength { len })
    }

    /// The chunk table's XXH3-64, `computed`, is not the header's `stored`.
    pub fn chunk_table(stored: u64, computed: u64) -> Self {
        Self(ProtocolErrorEnum::ChunkTable { stored, computed })
    }

    /// Chunk `index`, body bytes `start` to `end`, does not match its leaf.
    pub fn chunk(index: usize, start: usize, end: usize) -> Self {
        Self(ProtocolErrorEnum::Chunk { index, start, end })
    }

    /// A frame carries sequence `found` where `expected` was due.
    pub fn sequence_order(expected: u128, found: u128) -> Self {
        Self(ProtocolErrorEnum::SequenceOrder { expected, found })
    }

    /// A view of command `expected` was asked of a frame of command `found`.
    pub fn wrong_command(expected: u16, found: u16) -> Self {
        Self(ProtocolErrorEnum::WrongCommand { expected, found })
    }

    /// A body of command `command`, `len` bytes, is shorter than the `need` bytes of its fixed part and section table.
    pub fn body_too_short(command: u16, len: usize, need: usize) -> Self {
        Self(ProtocolErrorEnum::BodyTooShort { command, len, need })
    }

    /// Section `section`, `len` bytes at `offset`, runs past the body's `body` bytes.
    pub fn section_out_of_bounds(
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

    /// Section `section` starts at `offset`, not at `expected`.
    pub fn section_offset(command: u16, section: usize, offset: u32, expected: usize) -> Self {
        Self(ProtocolErrorEnum::SectionOffset {
            command,
            section,
            offset,
            expected,
        })
    }

    /// The padding byte at body offset `offset` is not zero.
    pub fn padding_not_zero(command: u16, offset: usize) -> Self {
        Self(ProtocolErrorEnum::PaddingNotZero { command, offset })
    }

    /// The body holds `len` bytes, past the end of its last section at `end`.
    pub fn trailing_bytes(command: u16, end: usize, len: usize) -> Self {
        Self(ProtocolErrorEnum::TrailingBytes { command, end, len })
    }

    /// Section `section`, `len` bytes, is not a whole number of `unit`-byte records.
    pub fn section_length(command: u16, section: usize, len: usize, unit: usize) -> Self {
        Self(ProtocolErrorEnum::SectionLength {
            command,
            section,
            len,
            unit,
        })
    }

    /// Entry `entry` of section `section`, at section offset `offset`, runs past the section.
    pub fn entry_out_of_bounds(command: u16, section: usize, entry: usize, offset: usize) -> Self {
        Self(ProtocolErrorEnum::EntryOutOfBounds {
            command,
            section,
            entry,
            offset,
        })
    }

    /// `field` counts `count` where the body holds `expected`.
    pub fn count_mismatch(command: u16, field: &'static str, count: u64, expected: u64) -> Self {
        Self(ProtocolErrorEnum::CountMismatch {
            command,
            field,
            count,
            expected,
        })
    }

    /// The text of section `section` is not UTF-8 at section offset `offset`.
    pub fn not_utf8(command: u16, section: usize, offset: usize) -> Self {
        Self(ProtocolErrorEnum::NotUtf8 {
            command,
            section,
            offset,
        })
    }

    /// Entry `at` of section `section` is `index`, past the `limit` entries it indexes.
    pub fn index_out_of_range(
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

    /// Term `at` of section `section` is not a term.
    pub fn bad_term(command: u16, section: usize, at: usize) -> Self {
        Self(ProtocolErrorEnum::BadTerm {
            command,
            section,
            at,
        })
    }

    /// `field` holds `value`, which the command does not define.
    pub fn field_value(command: u16, field: &'static str, value: u64) -> Self {
        Self(ProtocolErrorEnum::FieldValue {
            command,
            field,
            value,
        })
    }

    /// Entry `at` of the term table in section `section` repeats entry `earlier`.
    pub fn duplicate_term(command: u16, section: usize, at: usize, earlier: usize) -> Self {
        Self(ProtocolErrorEnum::DuplicateTerm {
            command,
            section,
            at,
            earlier,
        })
    }

    /// Entry `at` of the term table in section `section` is nested `depth` deep, past `limit`.
    pub fn term_too_deep(
        command: u16,
        section: usize,
        at: usize,
        depth: usize,
        limit: usize,
    ) -> Self {
        Self(ProtocolErrorEnum::TermTooDeep {
            command,
            section,
            at,
            depth,
            limit,
        })
    }

    /// Record `index`'s refusal `cause`, as a Round body reports it.
    pub fn record(index: usize, cause: ProtocolError) -> Self {
        Self(ProtocolErrorEnum::Record {
            index,
            cause: Box::new(cause.0),
        })
    }

    /// A Result or Entries frame on `stream`, past the stream's credit.
    pub fn past_credit(stream: u64) -> Self {
        Self(ProtocolErrorEnum::PastCredit { stream })
    }
}
