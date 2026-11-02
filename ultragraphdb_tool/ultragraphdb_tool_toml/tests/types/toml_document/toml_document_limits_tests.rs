/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Each bound of the parser, met and passed by one.

use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_tool_toml::TomlDocument;
use ultragraphdb_tool_toml::{TomlErrorEnum, TomlLimit};

use crate::utils::document::rows;
use crate::utils::refusal::{refusal, table_names};

/// A document of exactly `size` bytes: comment lines of 4,096 bytes, the last shorter, each ending in a line feed but
/// the last; every line within the line limit.
fn document_of(size: usize) -> Vec<u8> {
    let mut document = Vec::with_capacity(size);
    while document.len() < size {
        let left = size - document.len();
        let line = left.min(4_097);
        document.push(b'#');
        document.extend(std::iter::repeat_n(b'x', line.saturating_sub(2)));
        if line >= 2 {
            document.push(b'\n');
        }
    }
    document
}

#[test]
fn test_the_limits_are_those_of_the_spec() {
    // settings-toml, "The parser is bounded": 64 KiB, 4 KiB, 256 tables, 4,096 keys.
    assert_eq!(TomlDocument::BYTES_MAX, 65_536);
    assert_eq!(TomlDocument::LINE_BYTES_MAX, 4_096);
    assert_eq!(TomlDocument::TABLES_MAX, 256);
    assert_eq!(TomlDocument::KEYS_MAX, 4_096);
}

#[test]
fn test_a_document_of_64_kib_parses_and_one_byte_more_is_refused() -> TestResult {
    let at = document_of(65_536);
    assert_eq!(at.len(), 65_536);
    assert!(rows(&TomlDocument::parse(&at)?).is_empty());
    let past = document_of(65_537);
    assert_eq!(past.len(), 65_537);
    assert_eq!(
        refusal(&past)?,
        TomlErrorEnum::Limit {
            limit: TomlLimit::DocumentBytes,
            max: 65_536,
        }
    );
    Ok(())
}

#[test]
fn test_the_document_limit_comes_before_any_other_refusal() -> TestResult {
    let past = vec![0xFF; 65_537];
    assert_eq!(
        refusal(&past)?,
        TomlErrorEnum::Limit {
            limit: TomlLimit::DocumentBytes,
            max: 65_536,
        }
    );
    Ok(())
}

#[test]
fn test_a_line_of_4_kib_parses_and_one_byte_more_is_refused() -> TestResult {
    // `k = "` and `"` are 5 and 1 bytes, so 4,090 more make a line of 4,096.
    let line = |fill: usize| format!("k = \"{}\"", "x".repeat(fill));
    for ending in ["", "\n", "\r\n"] {
        let at = format!("a = 1\n{}{ending}", line(4_090));
        assert_eq!(
            rows(&TomlDocument::parse(at.as_bytes())?).len(),
            2,
            "{ending:?}"
        );
        let past = format!("a = 1\n{}{ending}", line(4_091));
        assert_eq!(
            refusal(past.as_bytes())?,
            TomlErrorEnum::Limit {
                limit: TomlLimit::LineBytes,
                max: 4_096,
            },
            "{ending:?}"
        );
    }
    Ok(())
}

#[test]
fn test_256_tables_parse_and_one_more_is_refused() -> TestResult {
    let tables = |count: usize| (0..count).map(|i| format!("[t{i}]\n")).collect::<String>();
    let document = TomlDocument::parse(tables(256).as_bytes())?;
    // The root table is not counted.
    assert_eq!(table_names(&document).len(), 257);
    assert_eq!(
        refusal(tables(257).as_bytes())?,
        TomlErrorEnum::Limit {
            limit: TomlLimit::Tables,
            max: 256,
        }
    );
    Ok(())
}

#[test]
fn test_4096_keys_parse_and_one_more_is_refused_in_every_table_together() -> TestResult {
    // A third in the root table and a third in each of two tables, so a count restarted at any header would allow
    // the 4,097th.
    let keys = |count: usize| {
        let mut document = String::new();
        for i in 0..count {
            if i == 1_365 {
                document.push_str("[t]\n");
            }
            if i == 2_730 {
                document.push_str("[u]\n");
            }
            document.push_str(&format!("k{i} = {i}\n"));
        }
        document
    };
    assert_eq!(
        rows(&TomlDocument::parse(keys(4_096).as_bytes())?).len(),
        4_096
    );
    assert_eq!(
        refusal(keys(4_097).as_bytes())?,
        TomlErrorEnum::Limit {
            limit: TomlLimit::Keys,
            max: 4_096,
        }
    );
    Ok(())
}
