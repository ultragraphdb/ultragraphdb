/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! `StringWriter` against the sections `PROTOCOL.md` §4.3 lays out, written by hand.

use ultragraphdb_core_body::{StringList, StringWriter};
use ultragraphdb_tool_bdd::TestResult;

/// "ab" ends at 2, "" at 2, "ü" at 4: ends 2, 2, 4 as little-endian u32s, the text "abü".
#[test]
fn test_pushed_strings_take_the_next_index_each() -> TestResult {
    let mut writer = StringWriter::default();
    assert_eq!(writer.push("ab"), 0);
    assert_eq!(writer.push(""), 1);
    assert_eq!(writer.push("ü"), 2);
    let (ends, text) = writer.sections();
    assert_eq!(ends, [2, 0, 0, 0, 2, 0, 0, 0, 4, 0, 0, 0]);
    assert_eq!(text, "abü".as_bytes());
    Ok(())
}

/// An interned string is written at its first use and named by that index after; pushing writes it again.
#[test]
fn test_an_interned_string_is_written_once() -> TestResult {
    let mut writer = StringWriter::default();
    assert_eq!(writer.intern("x"), 0);
    assert_eq!(writer.intern("y"), 1);
    assert_eq!(writer.intern("x"), 0);
    assert_eq!(writer.push("x"), 2);
    let (ends, text) = writer.sections();
    assert_eq!(ends, [1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0]);
    assert_eq!(text, b"xyx");
    Ok(())
}

/// No string written: two empty sections, an empty list.
#[test]
fn test_no_string_is_two_empty_sections() -> TestResult {
    let (ends, text) = StringWriter::default().sections();
    assert!(ends.is_empty());
    assert!(text.is_empty());
    Ok(())
}

/// What the writer writes, the reader reads back, string by string.
#[test]
fn test_the_written_list_reads_back() -> TestResult {
    let mut writer = StringWriter::default();
    let strings = ["<http://ex.org/a>", "", "ü", "<http://ex.org/a>"];
    for s in strings {
        let _index: u32 = writer.push(s);
    }
    let (ends, text) = writer.sections();
    let list = StringList::read((0, 1), &ends, &text)?;
    assert_eq!(list.iter().collect::<Vec<_>>(), strings);
    Ok(())
}
