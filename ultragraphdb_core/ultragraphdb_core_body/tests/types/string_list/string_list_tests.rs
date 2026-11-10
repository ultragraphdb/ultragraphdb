/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! `StringList` against lists laid out by hand from `PROTOCOL.md` §4.3: `ends`, one little-endian u32 per string, its
//! end within `text`; string i is `text[ends[i − 1] .. ends[i]]` with `ends[−1] = 0`.

use ultragraphdb_core_body::{BodyError, StringList};
use ultragraphdb_tool_bdd::{Context, TestResult};

fn ends(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// "a" ends at 1, "" at 1, "ü" (two bytes) at 3, "<http://ex.org/x>" (17 bytes) at 20.
#[test]
fn test_a_list_reads_its_strings_in_order() -> TestResult {
    let text = "aü<http://ex.org/x>";
    let ends = ends(&[1, 1, 3, 20]);
    let list = StringList::read((4, 5), &ends, text.as_bytes())?;
    assert_eq!(list.len(), 4);
    assert!(!list.is_empty());
    assert_eq!(list.get(0), Some("a"));
    assert_eq!(list.get(1), Some(""));
    assert_eq!(list.get(2), Some("ü"));
    assert_eq!(list.get(3), Some("<http://ex.org/x>"));
    assert_eq!(list.get(4), None);
    assert_eq!(list.get(usize::MAX), None);
    assert_eq!(
        list.iter().collect::<Vec<_>>(),
        vec!["a", "", "ü", "<http://ex.org/x>"]
    );
    Ok(())
}

/// An empty list is two empty sections, the list `EMPTY` is.
#[test]
fn test_an_empty_list_is_two_empty_sections() -> TestResult {
    let list = StringList::read((0, 1), &[], &[])?;
    assert!(list.is_empty());
    assert_eq!(list.len(), 0);
    assert_eq!(list.iter().count(), 0);
    assert_eq!(list, StringList::EMPTY);
    assert!(StringList::EMPTY.is_empty());
    Ok(())
}

/// Step 1: `ends` is a whole number of u32s, the refusal naming the ends' section.
#[test]
fn test_ends_are_whole_u32s() -> TestResult {
    assert_eq!(
        StringList::read((4, 5), &[2, 0, 0], b"ab")
            .err()
            .context("a refusal")?,
        BodyError::section_length(4, 3, 4)
    );
    Ok(())
}

/// Step 2: each end is at least the one before and at most the text's length, the refusal naming the entry.
#[test]
fn test_ends_never_decrease_and_stay_within_the_text() -> TestResult {
    assert_eq!(
        StringList::read((0, 1), &ends(&[3, 2]), b"abcd")
            .err()
            .context("a refusal")?,
        BodyError::index_out_of_range(0, 1, 2, 4)
    );
    assert_eq!(
        StringList::read((0, 1), &ends(&[5]), b"abcd")
            .err()
            .context("a refusal")?,
        BodyError::index_out_of_range(0, 0, 5, 4)
    );
    // An end equal to the one before, and the last at the text's length, hold.
    let held = ends(&[2, 2, 4]);
    let list = StringList::read((0, 1), &held, b"abcd")?;
    assert_eq!(list.get(1), Some(""));
    Ok(())
}

/// Step 3: the last end, or 0 for no strings, is the text's length.
#[test]
fn test_the_last_end_is_the_texts_length() -> TestResult {
    assert_eq!(
        StringList::read((0, 1), &ends(&[2]), b"abcd")
            .err()
            .context("a refusal")?,
        BodyError::count_mismatch("string text", 4, 2)
    );
    assert_eq!(
        StringList::read((0, 1), &[], b"abcd")
            .err()
            .context("a refusal")?,
        BodyError::count_mismatch("string text", 4, 0)
    );
    Ok(())
}

/// Steps 4 and 5: the text is UTF-8, and every end falls on a character boundary, the refusal naming the text's
/// section and the offset.
#[test]
fn test_every_string_is_utf8() -> TestResult {
    assert_eq!(
        StringList::read((2, 3), &ends(&[3]), &[b'a', 0xFF, b'b'])
            .err()
            .context("a refusal")?,
        BodyError::not_utf8(3, 1)
    );
    // "ü" is 0xC3 0xBC: an end of 1 splits it.
    assert_eq!(
        StringList::read((2, 3), &ends(&[1, 2]), "ü".as_bytes())
            .err()
            .context("a refusal")?,
        BodyError::not_utf8(3, 1)
    );
    Ok(())
}

/// The steps run in order: an end past the text is named before the text's invalid byte.
#[test]
fn test_the_first_rule_broken_is_named() -> TestResult {
    assert_eq!(
        StringList::read((0, 1), &ends(&[9]), &[0xFF])
            .err()
            .context("a refusal")?,
        BodyError::index_out_of_range(0, 0, 9, 1)
    );
    Ok(())
}
