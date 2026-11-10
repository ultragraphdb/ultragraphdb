/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! `Sections` against bodies laid out by hand from `PROTOCOL.md` §4.1's table: the section table at `F`, section 0 at
//! `align8(F + 8N)`, each later section at `align8` of the end of the one before, zero padding, nothing after.

use ultragraphdb_core_body::{BodyError, Sections, write_sections};
use ultragraphdb_tool_bdd::{Context, TestResult};

/// A body laid out by hand: `fixed`, then the table of `(offset, len)` pairs, then `rest`, the bytes after the table.
fn body(fixed: &[u8], table: &[(u32, u32)], rest: &[u8]) -> Vec<u8> {
    let mut out = fixed.to_vec();
    for (offset, len) in table {
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
    }
    out.extend_from_slice(rest);
    out
}

/// `PROTOCOL.md` §8.3: "The request's body, nine empty sections, is 72 bytes: the table, every `offset` 72 and every
/// `len` 0."
#[test]
fn test_nine_empty_sections_are_72_bytes_every_offset_72() -> TestResult {
    let bytes = body(&[], &[(72, 0); 9], &[]);
    assert_eq!(bytes.len(), 72);
    let sections = Sections::<9>::read(&bytes, 0)?;
    for index in 0..9 {
        assert_eq!(sections.get(index), b"");
        assert_eq!(sections.offset(index), 72);
    }
    assert_eq!(sections.fixed(), b"");
    assert_eq!(write_sections(&[], &[b"".as_slice(); 9]), bytes);
    Ok(())
}

/// Two sections of 3 and 2 bytes: section 0 at 16, the end of the table; section 1 at `align8(19)` = 24, after five
/// zero bytes; the body ends at 26.
#[test]
fn test_a_section_starts_at_the_next_multiple_of_8_after_the_one_before() -> TestResult {
    let mut rest = b"abc".to_vec();
    rest.extend_from_slice(&[0; 5]);
    rest.extend_from_slice(b"de");
    let bytes = body(&[], &[(16, 3), (24, 2)], &rest);
    assert_eq!(bytes.len(), 26);
    let sections = Sections::<2>::read(&bytes, 0)?;
    assert_eq!(sections.get(0), b"abc");
    assert_eq!(sections.get(1), b"de");
    assert_eq!(sections.offset(0), 16);
    assert_eq!(sections.offset(1), 24);
    // Past the last section: no bytes, and the body's end.
    assert_eq!(sections.get(2), b"");
    assert_eq!(sections.offset(2), 26);
    assert_eq!(
        write_sections(&[], &[b"abc".as_slice(), b"de".as_slice()]),
        bytes
    );
    Ok(())
}

/// A fixed part of 24 bytes, a resume key's (`PROTOCOL.md` §11.8): the table at 24, section 0 at 32.
#[test]
fn test_the_fixed_part_comes_before_the_table() -> TestResult {
    let fixed: Vec<u8> = (1..=24).collect();
    let bytes = body(&fixed, &[(32, 3)], b"xyz");
    let sections = Sections::<1>::read(&bytes, 24)?;
    assert_eq!(sections.fixed(), fixed.as_slice());
    assert_eq!(sections.get(0), b"xyz");
    assert_eq!(sections.offset(0), 32);
    assert_eq!(write_sections(&fixed, &[b"xyz".as_slice()]), bytes);
    Ok(())
}

/// A body of no sections is empty; one byte is past its end.
#[test]
fn test_a_body_of_no_sections_is_empty() -> TestResult {
    let sections = Sections::<0>::read(&[], 0)?;
    assert_eq!(sections.get(0), b"");
    assert_eq!(sections.offset(0), 0);
    assert_eq!(
        Sections::<0>::read(&[0], 0).err().context("a refusal")?,
        BodyError::trailing_bytes(0, 1)
    );
    Ok(())
}

/// Step 1: the fixed part and the table, 8 + 16 bytes, are present.
#[test]
fn test_a_body_shorter_than_its_table_is_refused() -> TestResult {
    let bytes = body(&[0; 8], &[(24, 0)], &[]);
    assert_eq!(
        Sections::<2>::read(&bytes, 8).err().context("a refusal")?,
        BodyError::body_too_short(16, 24)
    );
    Ok(())
}

/// Step 2, per section: within the body, at its offset, after zero padding.
#[test]
fn test_each_section_is_within_the_body_at_its_offset_after_zero_padding() -> TestResult {
    // Section 1 runs 2 bytes past the 26-byte body.
    let mut rest = b"abc".to_vec();
    rest.extend_from_slice(&[0; 5]);
    rest.extend_from_slice(b"de");
    let past = body(&[], &[(16, 3), (24, 4)], &rest);
    assert_eq!(
        Sections::<2>::read(&past, 0).err().context("a refusal")?,
        BodyError::section_out_of_bounds(1, 24, 4, 26)
    );
    // The largest offset and length: the sum saturates, never wraps, and is past the body.
    let huge = body(&[], &[(u32::MAX, u32::MAX)], &[]);
    assert_eq!(
        Sections::<1>::read(&huge, 0).err().context("a refusal")?,
        BodyError::section_out_of_bounds(0, u32::MAX, u32::MAX, 8)
    );
    // Section 1 at 20, not at align8(19) = 24.
    let misaligned = body(&[], &[(16, 3), (20, 2)], b"abc\0abcd");
    assert_eq!(
        Sections::<2>::read(&misaligned, 0)
            .err()
            .context("a refusal")?,
        BodyError::section_offset(1, 20, 24)
    );
    // Section 0 at 24, a gap after the table's end at 16.
    let gapped = body(&[], &[(24, 0)], &[0; 16]);
    assert_eq!(
        Sections::<1>::read(&gapped[..24], 0)
            .err()
            .context("a refusal")?,
        BodyError::section_offset(0, 24, 8)
    );
    // The padding byte at body offset 21 is 7.
    let mut padded = rest.clone();
    if let Some(byte) = padded.get_mut(5) {
        *byte = 7;
    }
    let dirty = body(&[], &[(16, 3), (24, 2)], &padded);
    assert_eq!(
        Sections::<2>::read(&dirty, 0).err().context("a refusal")?,
        BodyError::padding_not_zero(21)
    );
    Ok(())
}

/// Step 3: the last section ends at the body's end; and the steps run in order, the first broken named.
#[test]
fn test_bytes_after_the_last_section_are_refused_after_each_sections_rules() -> TestResult {
    let trailing = body(&[], &[(8, 1)], b"a\0");
    assert_eq!(
        Sections::<1>::read(&trailing, 0)
            .err()
            .context("a refusal")?,
        BodyError::trailing_bytes(9, 10)
    );
    // A wrong offset and trailing bytes: the offset, step 2, is named.
    let both = body(&[], &[(9, 1)], b"\0a\0");
    assert_eq!(
        Sections::<1>::read(&both, 0).err().context("a refusal")?,
        BodyError::section_offset(0, 9, 8)
    );
    Ok(())
}
