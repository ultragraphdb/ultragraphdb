/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Every body's layout: a fixed part, then a table of sections, each `offset` u32 and `len` u32 within the body, then
//! the sections. Section 0 starts at the end of the table and each later section at the end of the one before,
//! rounded up to 8 bytes; the padding is zero and the body ends where its last section ends. So a body has one
//! encoding, and offsets give a reader any section in place.

use ultragraphdb_error_protocol::ProtocolError;

use crate::utils::le::bytes_at;

/// The sections of a body checked against the layout, `N` of them, after a fixed part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Sections<'a, const N: usize> {
    body: &'a [u8],
    fixed: usize,
    ranges: [(usize, usize); N],
}

impl<'a, const N: usize> Sections<'a, N> {
    /// `body` checked as a fixed part of `fixed` bytes and `N` sections, in order: the fixed part and table present,
    /// refused as [`ProtocolError::BodyTooShort`]; for each section, its bytes inside the body
    /// ([`ProtocolError::SectionOutOfBounds`]), its offset the expected one ([`ProtocolError::SectionOffset`]) and the
    /// padding before it zero ([`ProtocolError::PaddingNotZero`]); then nothing after the last section
    /// ([`ProtocolError::TrailingBytes`]). `command` names the body in each error.
    pub(crate) fn read(command: u16, body: &'a [u8], fixed: usize) -> Result<Self, ProtocolError> {
        let need = fixed.saturating_add(8 * N);
        if body.len() < need {
            return Err(ProtocolError::BodyTooShort(command, body.len(), need));
        }
        let mut ranges = [(0usize, 0usize); N];
        let mut end = need;
        for (section, range) in ranges.iter_mut().enumerate() {
            let at = fixed + 8 * section;
            let offset = bytes_at(body, at).map_or(0, u32::from_le_bytes);
            let len = bytes_at(body, at + 4).map_or(0, u32::from_le_bytes);
            let start = offset as usize;
            let stop = start.saturating_add(len as usize);
            if stop > body.len() {
                return Err(ProtocolError::SectionOutOfBounds(
                    command,
                    section,
                    offset,
                    len,
                    body.len(),
                ));
            }
            let expected = align8(end);
            if start != expected {
                return Err(ProtocolError::SectionOffset(
                    command, section, offset, expected,
                ));
            }
            if let Some(i) = body
                .get(end..start)
                .and_then(|padding| padding.iter().position(|b| *b != 0))
            {
                return Err(ProtocolError::PaddingNotZero(command, end + i));
            }
            *range = (start, stop);
            end = stop;
        }
        if end != body.len() {
            return Err(ProtocolError::TrailingBytes(command, end, body.len()));
        }
        Ok(Self {
            body,
            fixed,
            ranges,
        })
    }

    /// The fixed part's bytes.
    pub(crate) fn fixed(&self) -> &'a [u8] {
        self.body.get(..self.fixed).unwrap_or_default()
    }

    /// Section `index`'s bytes; empty past the last section.
    pub(crate) fn get(&self, index: usize) -> &'a [u8] {
        self.ranges
            .get(index)
            .and_then(|(start, end)| self.body.get(*start..*end))
            .unwrap_or_default()
    }

    /// The body offset section `index` starts at; the body's length past the last section.
    pub(crate) fn offset(&self, index: usize) -> usize {
        self.ranges
            .get(index)
            .map_or(self.body.len(), |(start, _)| *start)
    }
}

/// The body of `fixed` and `sections` in this layout. `fixed` is padded with zeros to a multiple of 8.
pub(crate) fn write_sections(fixed: &[u8], sections: &[&[u8]]) -> Vec<u8> {
    let mut out = fixed.to_vec();
    out.resize(align8(fixed.len()), 0);
    let table = out.len();
    out.resize(table + 8 * sections.len(), 0);
    let mut entries = Vec::with_capacity(sections.len());
    for section in sections {
        out.resize(align8(out.len()), 0);
        entries.push((as_u32(out.len()), as_u32(section.len())));
        out.extend_from_slice(section);
    }
    let encoded: Vec<u8> = entries
        .iter()
        .flat_map(|(offset, len)| offset.to_le_bytes().into_iter().chain(len.to_le_bytes()))
        .collect();
    out.iter_mut()
        .skip(table)
        .zip(encoded)
        .for_each(|(byte, value)| *byte = value);
    out
}

/// `n` rounded up to a multiple of 8, saturating.
pub(crate) fn align8(n: usize) -> usize {
    n.div_ceil(8).saturating_mul(8)
}

/// The `unit`-byte records of section `section`, refused as [`ProtocolError::SectionLength`] when its length is not a
/// multiple of `unit`.
pub(crate) fn records<'a>(
    command: u16,
    section: usize,
    bytes: &'a [u8],
    unit: usize,
) -> Result<std::slice::ChunksExact<'a, u8>, ProtocolError> {
    if unit == 0 || !bytes.len().is_multiple_of(unit) {
        return Err(ProtocolError::SectionLength(
            command,
            section,
            bytes.len(),
            unit,
        ));
    }
    Ok(bytes.chunks_exact(unit))
}

/// The little-endian u32 at index `index` of an array of them, `None` past its end.
pub(crate) fn u32_at(bytes: &[u8], index: usize) -> Option<u32> {
    bytes_at(bytes, index.checked_mul(4)?).map(u32::from_le_bytes)
}

/// The little-endian u32s of a section, refused as [`ProtocolError::SectionLength`] when its length is not a multiple
/// of 4.
pub(crate) fn u32s(command: u16, section: usize, bytes: &[u8]) -> Result<Vec<u32>, ProtocolError> {
    Ok(records(command, section, bytes, 4)?
        .map(|chunk| chunk.try_into().map_or(0, u32::from_le_bytes))
        .collect())
}

/// `values` as little-endian u32s.
pub(crate) fn u32_bytes(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// A count or offset as the u32 the layout stores, saturating: a body past 4 GiB is refused as too large anyway.
pub(crate) fn as_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The UTF-8 text of section `section`, refused as [`ProtocolError::NotUtf8`] at its first invalid byte.
pub(crate) fn text(command: u16, section: usize, bytes: &[u8]) -> Result<&str, ProtocolError> {
    std::str::from_utf8(bytes)
        .map_err(|e| ProtocolError::NotUtf8(command, section, e.valid_up_to()))
}

/// The command header of `frame`, refused as [`ProtocolError::WrongCommand`] when the frame is not of `command`.
pub(crate) fn expect_command<'a>(
    frame: &crate::types::frame_view::FrameView<'a>,
    command: crate::types::command::Command,
) -> Result<crate::utils::command_header::CommandHeader<'a>, ProtocolError> {
    if frame.command() != command {
        return Err(ProtocolError::WrongCommand(
            command.number(),
            frame.command().number(),
        ));
    }
    Ok(crate::utils::command_header::CommandHeader(
        frame.header().command_header(),
    ))
}
