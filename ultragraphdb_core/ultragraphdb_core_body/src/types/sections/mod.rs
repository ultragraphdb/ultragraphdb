/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Every body's layout (`PROTOCOL.md` §4.1): a fixed part, then a table of sections, each `offset` u32 and `len` u32
//! within the body, then the sections. Section 0 starts at the end of the table and each later section at the end of
//! the one before, rounded up to 8 bytes; the padding is zero and the body ends where its last section ends. So a body
//! has one encoding, and offsets give a reader any section in place.

use crate::errors::body_error::BodyError;
use crate::utils::layout::align8;

/// The sections of a body checked against the layout, `N` of them, after a fixed part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sections<'a, const N: usize> {
    body: &'a [u8],
    fixed: usize,
    ranges: [(usize, usize); N],
}

impl<'a, const N: usize> Sections<'a, N> {
    /// `body` checked as a fixed part of `fixed` bytes and `N` sections, in order: the fixed part and table present,
    /// refused as [`BodyError::body_too_short`]; for each section, its bytes inside the body
    /// ([`BodyError::section_out_of_bounds`]), its offset the expected one ([`BodyError::section_offset`]) and the
    /// padding before it zero ([`BodyError::padding_not_zero`]); then nothing after the last section
    /// ([`BodyError::trailing_bytes`]).
    ///
    /// # Errors
    ///
    /// The first rule of `PROTOCOL.md` §4.1 the body breaks, in the order above.
    pub fn read(body: &'a [u8], fixed: usize) -> Result<Self, BodyError> {
        let need = fixed.saturating_add(N.saturating_mul(8));
        let Some(table) = body.get(fixed..need) else {
            return Err(BodyError::body_too_short(body.len(), need));
        };
        let mut ranges = [(0usize, 0usize); N];
        let mut end = need;
        let entries = table.as_chunks::<8>().0;
        for (section, (range, entry)) in ranges.iter_mut().zip(entries).enumerate() {
            let [o0, o1, o2, o3, l0, l1, l2, l3] = *entry;
            let offset = u32::from_le_bytes([o0, o1, o2, o3]);
            let len = u32::from_le_bytes([l0, l1, l2, l3]);
            let start = offset as usize;
            let stop = start.saturating_add(len as usize);
            if stop > body.len() {
                return Err(BodyError::section_out_of_bounds(
                    section,
                    offset,
                    len,
                    body.len(),
                ));
            }
            let expected = align8(end);
            if start != expected {
                return Err(BodyError::section_offset(section, offset, expected));
            }
            if let Some((at, _)) = body
                .iter()
                .enumerate()
                .take(start)
                .skip(end)
                .find(|(_, b)| **b != 0)
            {
                return Err(BodyError::padding_not_zero(at));
            }
            *range = (start, stop);
            end = stop;
        }
        if end != body.len() {
            return Err(BodyError::trailing_bytes(end, body.len()));
        }
        Ok(Self {
            body,
            fixed,
            ranges,
        })
    }

    /// The fixed part's bytes.
    #[must_use]
    pub fn fixed(&self) -> &'a [u8] {
        self.body.get(..self.fixed).unwrap_or_default()
    }

    /// Section `index`'s bytes; empty past the last section.
    #[must_use]
    pub fn get(&self, index: usize) -> &'a [u8] {
        self.ranges
            .get(index)
            .and_then(|(start, end)| self.body.get(*start..*end))
            .unwrap_or_default()
    }

    /// The body offset section `index` starts at; the body's length past the last section.
    #[must_use]
    pub fn offset(&self, index: usize) -> usize {
        self.ranges
            .get(index)
            .map_or(self.body.len(), |(start, _)| *start)
    }
}
