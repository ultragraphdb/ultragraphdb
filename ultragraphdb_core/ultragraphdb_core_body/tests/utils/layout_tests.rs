/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The layout's helpers against values worked by hand: `align8` of `PROTOCOL.md` §4.1, little-endian u32s of §1.

use ultragraphdb_core_body::{
    BodyError, align8, as_u32, records, text, u32_at, u32_bytes, u32s, write_sections_into,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

/// Rounded up to a multiple of 8: 0, 1, 8, 9; the largest `usize` saturates.
#[test]
fn test_align8_rounds_up_to_a_multiple_of_8() -> TestResult {
    assert_eq!(align8(0), 0);
    assert_eq!(align8(1), 8);
    assert_eq!(align8(8), 8);
    assert_eq!(align8(9), 16);
    assert_eq!(align8(usize::MAX), usize::MAX);
    Ok(())
}

/// A count within a u32 is itself; one past it saturates.
#[test]
fn test_as_u32_saturates() -> TestResult {
    assert_eq!(as_u32(5), 5);
    assert_eq!(as_u32(u32::MAX as usize), u32::MAX);
    assert_eq!(as_u32(u32::MAX as usize + 1), u32::MAX);
    Ok(())
}

/// The u32 at an index of a little-endian array: 1, then 0x01020304; nothing past the end or at an index whose byte
/// offset overflows.
#[test]
fn test_u32s_read_and_write_little_endian() -> TestResult {
    let bytes = [1, 0, 0, 0, 4, 3, 2, 1];
    assert_eq!(u32_bytes(&[1, 0x0102_0304]), bytes);
    assert_eq!(u32_at(&bytes, 0), Some(1));
    assert_eq!(u32_at(&bytes, 1), Some(0x0102_0304));
    assert_eq!(u32_at(&bytes, 2), None);
    assert_eq!(u32_at(&bytes, usize::MAX), None);
    assert_eq!(u32s(3, &bytes)?, vec![1, 0x0102_0304]);
    assert_eq!(u32s(3, &[])?, Vec::<u32>::new());
    assert_eq!(
        u32s(3, &bytes[..5]).err().context("a refusal")?,
        BodyError::section_length(3, 5, 4)
    );
    Ok(())
}

/// A section of 8 bytes is two 4-byte records; one of 7 is refused, and so is a record of no bytes.
#[test]
fn test_a_section_is_a_whole_number_of_records() -> TestResult {
    let bytes = [0u8; 8];
    assert_eq!(records(2, &bytes, 4)?.count(), 2);
    assert_eq!(
        records(2, &bytes[..7], 4).err().context("a refusal")?,
        BodyError::section_length(2, 7, 4)
    );
    assert_eq!(
        records(2, &bytes, 0).err().context("a refusal")?,
        BodyError::section_length(2, 8, 0)
    );
    Ok(())
}

/// A text section is UTF-8, refused at its first invalid byte: `a` then a lone 0xC3, at offset 1.
#[test]
fn test_a_text_section_is_utf8() -> TestResult {
    assert_eq!(text(0, "ü".as_bytes())?, "ü");
    assert_eq!(
        text(6, &[b'a', 0xC3]).err().context("a refusal")?,
        BodyError::not_utf8(6, 1)
    );
    Ok(())
}

/// Written into a buffer holding an earlier body: cleared first, its capacity kept, a fixed part of 5 bytes padded to
/// 8, the table at 8, section 0 at 16.
#[test]
fn test_a_body_is_written_into_a_cleared_buffer() -> TestResult {
    let mut out = vec![0xEE; 64];
    let capacity = out.capacity();
    write_sections_into(&mut out, b"fixed", &[b"ab".as_slice()]);
    let mut expected = b"fixed\0\0\0".to_vec();
    expected.extend_from_slice(&16u32.to_le_bytes());
    expected.extend_from_slice(&2u32.to_le_bytes());
    expected.extend_from_slice(b"ab");
    assert_eq!(out, expected);
    assert!(out.capacity() >= capacity);
    Ok(())
}
