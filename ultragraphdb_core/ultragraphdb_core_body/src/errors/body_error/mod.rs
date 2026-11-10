/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod body_error_display;
mod body_error_error;

/// Why bytes were refused as a body of `PROTOCOL.md` §4.1's layout, or as a string list of §4.3: one variant per
/// rule, named as the protocol names its refusal. A body's reader names no command or entry: the wire adds the command
/// a body belongs to, the catalog the entry, when each carries the refusal on.
///
/// The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BodyError(pub BodyErrorEnum);

/// The classification of a [`BodyError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BodyErrorEnum {
    /// The body, `len` bytes, is shorter than its fixed part and section table, `need` bytes.
    BodyTooShort {
        /// The body's length in bytes.
        len: usize,
        /// The length of the body's fixed part and section table.
        need: usize,
    },
    /// Section `section`, `len` bytes at `offset`, runs past the body's `body` bytes.
    SectionOutOfBounds {
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
        /// The section's index in the body's section table.
        section: usize,
        /// The offset the section table gives.
        offset: u32,
        /// The offset due: the end of the part before, rounded up to 8 bytes.
        expected: usize,
    },
    /// The padding byte at body offset `offset` is not zero.
    PaddingNotZero {
        /// The body offset of the non-zero padding byte.
        offset: usize,
    },
    /// The body holds `len` bytes, past the end of its last section at `end`.
    TrailingBytes {
        /// The end of the body's last section.
        end: usize,
        /// The body's length in bytes.
        len: usize,
    },
    /// Section `section`, `len` bytes, is not a whole number of `unit`-byte records.
    SectionLength {
        /// The section's index in the body's section table.
        section: usize,
        /// The section's length in bytes.
        len: usize,
        /// The size of one record in bytes.
        unit: usize,
    },
    /// The count `field` holds, `count`, is not the count the body holds, `expected`.
    CountMismatch {
        /// The field holding the count.
        field: &'static str,
        /// The count the field holds.
        count: u64,
        /// The count the body holds.
        expected: u64,
    },
    /// The text of section `section` is not UTF-8 at section offset `offset`.
    NotUtf8 {
        /// The section's index in the body's section table.
        section: usize,
        /// The section offset of the first byte that is not UTF-8.
        offset: usize,
    },
    /// Entry `at` of section `section` is `index`, past the `limit` it may reach.
    IndexOutOfRange {
        /// The section's index in the body's section table.
        section: usize,
        /// The entry's index within the section.
        at: usize,
        /// The value the entry holds.
        index: u32,
        /// The bound the entry may reach.
        limit: u32,
    },
}

impl BodyError {
    /// The body, `len` bytes, is shorter than its fixed part and section table, `need` bytes.
    #[must_use]
    pub fn body_too_short(len: usize, need: usize) -> Self {
        Self(BodyErrorEnum::BodyTooShort { len, need })
    }

    /// Section `section`, `len` bytes at `offset`, runs past the body's `body` bytes.
    #[must_use]
    pub fn section_out_of_bounds(section: usize, offset: u32, len: u32, body: usize) -> Self {
        Self(BodyErrorEnum::SectionOutOfBounds {
            section,
            offset,
            len,
            body,
        })
    }

    /// Section `section` starts at `offset`, not at `expected`.
    #[must_use]
    pub fn section_offset(section: usize, offset: u32, expected: usize) -> Self {
        Self(BodyErrorEnum::SectionOffset {
            section,
            offset,
            expected,
        })
    }

    /// The padding byte at body offset `offset` is not zero.
    #[must_use]
    pub fn padding_not_zero(offset: usize) -> Self {
        Self(BodyErrorEnum::PaddingNotZero { offset })
    }

    /// The body holds `len` bytes, past the end of its last section at `end`.
    #[must_use]
    pub fn trailing_bytes(end: usize, len: usize) -> Self {
        Self(BodyErrorEnum::TrailingBytes { end, len })
    }

    /// Section `section`, `len` bytes, is not a whole number of `unit`-byte records.
    #[must_use]
    pub fn section_length(section: usize, len: usize, unit: usize) -> Self {
        Self(BodyErrorEnum::SectionLength { section, len, unit })
    }

    /// The count `field` holds, `count`, is not the count the body holds, `expected`.
    #[must_use]
    pub fn count_mismatch(field: &'static str, count: u64, expected: u64) -> Self {
        Self(BodyErrorEnum::CountMismatch {
            field,
            count,
            expected,
        })
    }

    /// The text of section `section` is not UTF-8 at section offset `offset`.
    #[must_use]
    pub fn not_utf8(section: usize, offset: usize) -> Self {
        Self(BodyErrorEnum::NotUtf8 { section, offset })
    }

    /// Entry `at` of section `section` is `index`, past the `limit` it may reach.
    #[must_use]
    pub fn index_out_of_range(section: usize, at: usize, index: u32, limit: u32) -> Self {
        Self(BodyErrorEnum::IndexOutOfRange {
            section,
            at,
            index,
            limit,
        })
    }
}
