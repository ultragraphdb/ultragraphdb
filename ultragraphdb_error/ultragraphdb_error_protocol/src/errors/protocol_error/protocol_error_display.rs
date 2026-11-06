/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::protocol_error::{ProtocolError, ProtocolErrorEnum};

use ProtocolErrorEnum as E;

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            e @ E::ShortHeader { .. } => short_header(e, f),
            e @ E::HeaderChecksum { .. } => header_checksum(e, f),
            e @ E::UnknownCommand { .. } => unknown_command(e, f),
            e @ E::CommandNotServed { .. } => command_not_served(e, f),
            e @ E::ReservedNotZero { .. } => reserved_not_zero(e, f),
            e @ E::WrongCluster { .. } => wrong_cluster(e, f),
            e @ E::SizeBelowHeader { .. } => size_below_header(e, f),
            e @ E::FrameTooLarge { .. } => frame_too_large(e, f),
            e @ E::LengthMismatch { .. } => length_mismatch(e, f),
            e @ E::BodyLength { .. } => body_length(e, f),
            e @ E::ChunkTable { .. } => chunk_table(e, f),
            e @ E::Chunk { .. } => chunk(e, f),
            e @ E::SequenceOrder { .. } => sequence_order(e, f),
            e @ E::WrongCommand { .. } => wrong_command(e, f),
            e @ E::BodyTooShort { .. } => body_too_short(e, f),
            e @ E::SectionOutOfBounds { .. } => section_out_of_bounds(e, f),
            e @ E::SectionOffset { .. } => section_offset(e, f),
            e @ E::PaddingNotZero { .. } => padding_not_zero(e, f),
            e @ E::TrailingBytes { .. } => trailing_bytes(e, f),
            e @ E::SectionLength { .. } => section_length(e, f),
            e @ E::EntryOutOfBounds { .. } => entry_out_of_bounds(e, f),
            e @ E::CountMismatch { .. } => count_mismatch(e, f),
            e @ E::NotUtf8 { .. } => not_utf8(e, f),
            e @ E::IndexOutOfRange { .. } => index_out_of_range(e, f),
            e @ E::BadTerm { .. } => bad_term(e, f),
            e @ E::FieldValue { .. } => field_value(e, f),
            e @ E::DuplicateTerm { .. } => duplicate_term(e, f),
            e @ E::TermTooDeep { .. } => term_too_deep(e, f),
            e @ E::Record { .. } => record(e, f),
            E::PastCredit { stream } => {
                write!(f, "a frame on stream {stream} past the stream's credit")
            }
        }
    }
}

/// The text of [`ProtocolErrorEnum::ShortHeader`].
fn short_header(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::ShortHeader { len } = e else {
        return Err(fmt::Error);
    };
    write!(
        f,
        "frame of {len} bytes is shorter than its 256-byte header"
    )
}

/// The text of [`ProtocolErrorEnum::HeaderChecksum`].
fn header_checksum(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::HeaderChecksum { stored, computed } = e else {
        return Err(fmt::Error);
    };
    write!(
        f,
        "header checksum {stored:#018x} does not match the computed {computed:#018x}"
    )
}

/// The text of [`ProtocolErrorEnum::UnknownCommand`].
fn unknown_command(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::UnknownCommand { command } = e else {
        return Err(fmt::Error);
    };
    write!(f, "unknown command {command}")
}

/// The text of [`ProtocolErrorEnum::CommandNotServed`].
fn command_not_served(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::CommandNotServed { command } = e else {
        return Err(fmt::Error);
    };
    write!(f, "command {command} is not served on this connection")
}

/// The text of [`ProtocolErrorEnum::ReservedNotZero`].
fn reserved_not_zero(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::ReservedNotZero { offset } = e else {
        return Err(fmt::Error);
    };
    write!(f, "reserved header byte {offset} is not zero")
}

/// The text of [`ProtocolErrorEnum::WrongCluster`].
fn wrong_cluster(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::WrongCluster { expected, found } = e else {
        return Err(fmt::Error);
    };
    write!(
        f,
        "frame of cluster {found:#018x}, not this cluster {expected:#018x}"
    )
}

/// The text of [`ProtocolErrorEnum::SizeBelowHeader`].
fn size_below_header(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::SizeBelowHeader { size } = e else {
        return Err(fmt::Error);
    };
    write!(f, "frame size {size} is below its 256-byte header")
}

/// The text of [`ProtocolErrorEnum::FrameTooLarge`].
fn frame_too_large(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::FrameTooLarge { size, limit } = e else {
        return Err(fmt::Error);
    };
    write!(f, "frame of {size} bytes exceeds the {limit}-byte limit")
}

/// The text of [`ProtocolErrorEnum::LengthMismatch`].
fn length_mismatch(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::LengthMismatch { size, len } = e else {
        return Err(fmt::Error);
    };
    write!(
        f,
        "frame size {size} does not match the {len} bytes received"
    )
}

/// The text of [`ProtocolErrorEnum::BodyLength`].
fn body_length(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::BodyLength { len } = e else {
        return Err(fmt::Error);
    };
    write!(
        f,
        "body of {len} bytes has no split into chunks and their table"
    )
}

/// The text of [`ProtocolErrorEnum::ChunkTable`].
fn chunk_table(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::ChunkTable { stored, computed } = e else {
        return Err(fmt::Error);
    };
    write!(
        f,
        "chunk table checksum {computed:#018x} does not match checksum_body {stored:#018x}"
    )
}

/// The text of [`ProtocolErrorEnum::Chunk`].
fn chunk(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::Chunk { index, start, end } = e else {
        return Err(fmt::Error);
    };
    write!(
        f,
        "chunk {index}, body bytes {start}..{end}, does not match its leaf"
    )
}

/// The text of [`ProtocolErrorEnum::SequenceOrder`].
fn sequence_order(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::SequenceOrder { expected, found } = e else {
        return Err(fmt::Error);
    };
    write!(
        f,
        "frame of sequence {found} where sequence {expected} was due"
    )
}

/// The text of [`ProtocolErrorEnum::WrongCommand`].
fn wrong_command(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::WrongCommand { expected, found } = e else {
        return Err(fmt::Error);
    };
    write!(
        f,
        "a view of command {expected} was asked of a frame of command {found}"
    )
}

/// The text of [`ProtocolErrorEnum::BodyTooShort`].
fn body_too_short(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::BodyTooShort { command, len, need } = e else {
        return Err(fmt::Error);
    };
    write!(
        f,
        "body of command {command}, {len} bytes, is shorter than its {need}-byte fixed part and section table"
    )
}

/// The text of [`ProtocolErrorEnum::SectionOutOfBounds`].
fn section_out_of_bounds(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::SectionOutOfBounds {
        command,
        section,
        offset,
        len,
        body,
    } = e
    else {
        return Err(fmt::Error);
    };
    write!(
        f,
        "section {section} of command {command}, {len} bytes at {offset}, runs past the {body}-byte body"
    )
}

/// The text of [`ProtocolErrorEnum::SectionOffset`].
fn section_offset(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::SectionOffset {
        command,
        section,
        offset,
        expected,
    } = e
    else {
        return Err(fmt::Error);
    };
    write!(
        f,
        "section {section} of command {command} starts at {offset}, not at {expected}"
    )
}

/// The text of [`ProtocolErrorEnum::PaddingNotZero`].
fn padding_not_zero(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::PaddingNotZero { command, offset } = e else {
        return Err(fmt::Error);
    };
    write!(
        f,
        "padding byte {offset} of a body of command {command} is not zero"
    )
}

/// The text of [`ProtocolErrorEnum::TrailingBytes`].
fn trailing_bytes(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::TrailingBytes { command, end, len } = e else {
        return Err(fmt::Error);
    };
    write!(
        f,
        "body of command {command} holds {len} bytes, past the end of its last section at {end}"
    )
}

/// The text of [`ProtocolErrorEnum::SectionLength`].
fn section_length(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::SectionLength {
        command,
        section,
        len,
        unit,
    } = e
    else {
        return Err(fmt::Error);
    };
    write!(
        f,
        "section {section} of command {command}, {len} bytes, is not a whole number of {unit}-byte records"
    )
}

/// The text of [`ProtocolErrorEnum::EntryOutOfBounds`].
fn entry_out_of_bounds(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::EntryOutOfBounds {
        command,
        section,
        entry,
        offset,
    } = e
    else {
        return Err(fmt::Error);
    };
    write!(
        f,
        "entry {entry} of section {section} of command {command}, at {offset}, runs past its section"
    )
}

/// The text of [`ProtocolErrorEnum::CountMismatch`].
fn count_mismatch(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::CountMismatch {
        command,
        field,
        count,
        expected,
    } = e
    else {
        return Err(fmt::Error);
    };
    write!(
        f,
        "{field} of command {command} counts {count} where the body holds {expected}"
    )
}

/// The text of [`ProtocolErrorEnum::NotUtf8`].
fn not_utf8(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::NotUtf8 {
        command,
        section,
        offset,
    } = e
    else {
        return Err(fmt::Error);
    };
    write!(
        f,
        "section {section} of command {command} is not UTF-8 at {offset}"
    )
}

/// The text of [`ProtocolErrorEnum::IndexOutOfRange`].
fn index_out_of_range(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::IndexOutOfRange {
        command,
        section,
        at,
        index,
        limit,
    } = e
    else {
        return Err(fmt::Error);
    };
    write!(
        f,
        "entry {at} of section {section} of command {command} is {index}, past the {limit} it indexes"
    )
}

/// The text of [`ProtocolErrorEnum::BadTerm`].
fn bad_term(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::BadTerm {
        command,
        section,
        at,
    } = e
    else {
        return Err(fmt::Error);
    };
    write!(
        f,
        "term {at} of section {section} of command {command} is not a term of its place"
    )
}

/// The text of [`ProtocolErrorEnum::FieldValue`].
fn field_value(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::FieldValue {
        command,
        field,
        value,
    } = e
    else {
        return Err(fmt::Error);
    };
    write!(
        f,
        "{field} of command {command} holds {value}, which the command does not define"
    )
}

/// The text of [`ProtocolErrorEnum::DuplicateTerm`].
fn duplicate_term(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::DuplicateTerm {
        command,
        section,
        at,
        earlier,
    } = e
    else {
        return Err(fmt::Error);
    };
    write!(
        f,
        "entry {at} of the term table in section {section} of command {command} repeats entry {earlier}"
    )
}

/// The text of [`ProtocolErrorEnum::TermTooDeep`].
fn term_too_deep(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::TermTooDeep {
        command,
        section,
        at,
        depth,
        limit,
    } = e
    else {
        return Err(fmt::Error);
    };
    write!(
        f,
        "entry {at} of the term table in section {section} of command {command} nests {depth} deep, past the {limit} allowed"
    )
}

/// The text of [`ProtocolErrorEnum::Record`].
fn record(e: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // Called only from the arm that matched this variant, so the `else` is never taken.
    let E::Record { index, cause } = e else {
        return Err(fmt::Error);
    };
    write!(
        f,
        "write-set record {index} of the round: {}",
        ProtocolError(cause.as_ref().clone())
    )
}
