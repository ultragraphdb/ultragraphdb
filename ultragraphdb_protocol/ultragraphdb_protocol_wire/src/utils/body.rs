/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The body layout and the string list of `ultragraphdb_core_body`, read for a body of a command: each refusal names
//! the command, as the protocol's refusals do (`PROTOCOL.md` §4.1, §4.3).

use ultragraphdb_core_body::{BodyError, BodyErrorEnum, Sections, StringList};
use ultragraphdb_error_protocol::ProtocolError;

/// `error`, a body of `command` refused by the layout, as the protocol's refusal naming the command.
pub(crate) fn refusal(command: u16, error: BodyError) -> ProtocolError {
    match error.0 {
        BodyErrorEnum::BodyTooShort { len, need } => {
            ProtocolError::body_too_short(command, len, need)
        }
        BodyErrorEnum::SectionOutOfBounds {
            section,
            offset,
            len,
            body,
        } => ProtocolError::section_out_of_bounds(command, section, offset, len, body),
        BodyErrorEnum::SectionOffset {
            section,
            offset,
            expected,
        } => ProtocolError::section_offset(command, section, offset, expected),
        BodyErrorEnum::PaddingNotZero { offset } => {
            ProtocolError::padding_not_zero(command, offset)
        }
        BodyErrorEnum::TrailingBytes { end, len } => {
            ProtocolError::trailing_bytes(command, end, len)
        }
        BodyErrorEnum::SectionLength { section, len, unit } => {
            ProtocolError::section_length(command, section, len, unit)
        }
        BodyErrorEnum::CountMismatch {
            field,
            count,
            expected,
        } => ProtocolError::count_mismatch(command, field, count, expected),
        BodyErrorEnum::NotUtf8 { section, offset } => {
            ProtocolError::not_utf8(command, section, offset)
        }
        BodyErrorEnum::IndexOutOfRange {
            section,
            at,
            index,
            limit,
        } => ProtocolError::index_out_of_range(command, section, at, index, limit),
    }
}

/// `body` of `command` read as [`Sections::read`] reads it, its refusal naming the command.
pub(crate) fn read_sections<const N: usize>(
    command: u16,
    body: &[u8],
    fixed: usize,
) -> Result<Sections<'_, N>, ProtocolError> {
    Sections::read(body, fixed).map_err(|e| refusal(command, e))
}

/// The string list of sections `sections` of a body of `command`, read as [`StringList::read`] reads it, its refusal
/// naming the command.
pub(crate) fn read_string_list<'a>(
    command: u16,
    sections: (usize, usize),
    ends: &'a [u8],
    text: &'a [u8],
) -> Result<StringList<'a>, ProtocolError> {
    StringList::read(sections, ends, text).map_err(|e| refusal(command, e))
}

/// The `unit`-byte records of section `section` of a body of `command`, as [`ultragraphdb_core_body::records`] reads
/// them, its refusal naming the command.
pub(crate) fn records(
    command: u16,
    section: usize,
    bytes: &[u8],
    unit: usize,
) -> Result<std::slice::ChunksExact<'_, u8>, ProtocolError> {
    ultragraphdb_core_body::records(section, bytes, unit).map_err(|e| refusal(command, e))
}

/// The u32s of section `section` of a body of `command`, as [`ultragraphdb_core_body::u32s`] reads them, its refusal
/// naming the command.
pub(crate) fn u32s(command: u16, section: usize, bytes: &[u8]) -> Result<Vec<u32>, ProtocolError> {
    ultragraphdb_core_body::u32s(section, bytes).map_err(|e| refusal(command, e))
}

/// The UTF-8 text of section `section` of a body of `command`, as [`ultragraphdb_core_body::text`] reads it, its
/// refusal naming the command.
pub(crate) fn text(command: u16, section: usize, bytes: &[u8]) -> Result<&str, ProtocolError> {
    ultragraphdb_core_body::text(section, bytes).map_err(|e| refusal(command, e))
}
