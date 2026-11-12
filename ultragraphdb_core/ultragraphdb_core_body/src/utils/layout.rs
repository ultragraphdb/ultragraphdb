/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The body layout's writer and the readers of a section's records, u32s and text (`PROTOCOL.md` §4.1).

use crate::errors::body_error::BodyError;
use crate::utils::le::bytes_at;

/// The body of `fixed` and `sections` in the layout of `PROTOCOL.md` §4.1. `fixed` is padded with zeros to a multiple
/// of 8.
#[must_use]
pub fn write_sections(fixed: &[u8], sections: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    write_sections_into(&mut out, fixed, sections);
    out
}

/// Writes the body of `fixed` and `sections` into `out`, cleared first and its capacity kept, as [`write_sections`] does.
pub fn write_sections_into(out: &mut Vec<u8>, fixed: &[u8], sections: &[&[u8]]) {
    out.clear();
    out.extend_from_slice(fixed);
    out.resize(align8(fixed.len()), 0);
    let table = out.len();
    sections.iter().for_each(|_| out.extend_from_slice(&[0; 8]));
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
}

/// `n` rounded up to a multiple of 8, saturating.
#[must_use]
pub fn align8(n: usize) -> usize {
    n.div_ceil(8).saturating_mul(8)
}

/// The `unit`-byte records of section `section`, refused as [`BodyError::section_length`] when its length is not a
/// multiple of `unit`.
///
/// # Errors
///
/// [`BodyError::section_length`] when `bytes` is not a whole number of `unit`-byte records, or `unit` is 0.
pub fn records(
    section: usize,
    bytes: &[u8],
    unit: usize,
) -> Result<std::slice::ChunksExact<'_, u8>, BodyError> {
    if unit == 0 || !bytes.len().is_multiple_of(unit) {
        return Err(BodyError::section_length(section, bytes.len(), unit));
    }
    Ok(bytes.chunks_exact(unit))
}

/// The little-endian u32 at index `index` of an array of them, `None` past its end.
#[must_use]
pub fn u32_at(bytes: &[u8], index: usize) -> Option<u32> {
    bytes_at(bytes, index.checked_mul(4)?).map(u32::from_le_bytes)
}

/// The little-endian u32s of section `section`, refused as [`BodyError::section_length`] when its length is not a
/// multiple of 4.
///
/// # Errors
///
/// [`BodyError::section_length`] when `bytes` is not a whole number of u32s.
pub fn u32s(section: usize, bytes: &[u8]) -> Result<Vec<u32>, BodyError> {
    Ok(records(section, bytes, 4)?
        .map(|chunk| chunk.try_into().map_or(0, u32::from_le_bytes))
        .collect())
}

/// `values` as little-endian u32s.
#[must_use]
pub fn u32_bytes(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// A count or offset as the u32 the layout stores, saturating: a body past 4 GiB is refused as too large anyway.
#[must_use]
pub fn as_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The UTF-8 text of section `section`, refused as [`BodyError::not_utf8`] at its first invalid byte.
///
/// # Errors
///
/// [`BodyError::not_utf8`] at the section offset of the first byte that is not UTF-8.
pub fn text(section: usize, bytes: &[u8]) -> Result<&str, BodyError> {
    std::str::from_utf8(bytes).map_err(|e| BodyError::not_utf8(section, e.valid_up_to()))
}
