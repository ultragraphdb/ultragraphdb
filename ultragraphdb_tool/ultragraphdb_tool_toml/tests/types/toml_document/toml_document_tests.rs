/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The document's structure: tables, keys, whitespace, comments and line endings, as TOML 1.0 reads them.

use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_tool_toml::{TomlDocument, TomlValue};

use crate::utils::document::{row, rows};
use crate::utils::refusal::table_names;

#[test]
fn test_an_empty_document_holds_the_empty_root_table() -> TestResult {
    let document = TomlDocument::parse(b"")?;
    assert_eq!(table_names(&document), [""]);
    assert!(rows(&document).is_empty());
    Ok(())
}

#[test]
fn test_blank_and_comment_only_lines_hold_nothing() -> TestResult {
    let document = TomlDocument::parse(b"\n# a comment\n   \n\t# indented \"quoted\" # twice\n\n")?;
    assert_eq!(table_names(&document), [""]);
    assert!(rows(&document).is_empty());
    Ok(())
}

#[test]
fn test_keys_before_any_header_are_in_the_root_table() -> TestResult {
    let document = TomlDocument::parse(b"name = \"dev\"\nretries = 3\n[storage]\nsync = false\n")?;
    assert_eq!(table_names(&document), ["", "storage"]);
    assert_eq!(
        rows(&document),
        [
            row("", "name", TomlValue::String("dev".to_string())),
            row("", "retries", TomlValue::Integer(3)),
            row("storage", "sync", TomlValue::Boolean(false)),
        ]
    );
    Ok(())
}

#[test]
fn test_tables_keep_the_order_of_their_headers_and_may_be_empty() -> TestResult {
    let document = TomlDocument::parse(b"[b]\n[a]\nx = 1\n[c]\n")?;
    assert_eq!(table_names(&document), ["", "b", "a", "c"]);
    assert_eq!(rows(&document), [row("a", "x", TomlValue::Integer(1))]);
    Ok(())
}

#[test]
fn test_bare_keys_take_letters_digits_underscore_and_hyphen() -> TestResult {
    let document =
        TomlDocument::parse(b"A-z_09 = 1\n1234 = 2\n-_- = 3\ntrue = 4\n[Tab-le_1]\nfalse = 5\n")?;
    assert_eq!(
        rows(&document),
        [
            row("", "A-z_09", TomlValue::Integer(1)),
            row("", "1234", TomlValue::Integer(2)),
            row("", "-_-", TomlValue::Integer(3)),
            row("", "true", TomlValue::Integer(4)),
            row("Tab-le_1", "false", TomlValue::Integer(5)),
        ]
    );
    Ok(())
}

#[test]
fn test_spaces_and_tabs_are_ignored_where_toml_allows_them() -> TestResult {
    let document = TomlDocument::parse(b"  a=1\n\tb\t=\t2\t\n[ \tt\t ]  \n  c =   true  # c\n")?;
    assert_eq!(
        rows(&document),
        [
            row("", "a", TomlValue::Integer(1)),
            row("", "b", TomlValue::Integer(2)),
            row("t", "c", TomlValue::Boolean(true)),
        ]
    );
    Ok(())
}

#[test]
fn test_comments_follow_values_and_headers() -> TestResult {
    let document = TomlDocument::parse(
        "[t] # the table\nk = \"# not a comment\" # é, \"quotes\", tab\t\n".as_bytes(),
    )?;
    assert_eq!(
        rows(&document),
        [row(
            "t",
            "k",
            TomlValue::String("# not a comment".to_string())
        )]
    );
    Ok(())
}

#[test]
fn test_lines_end_in_lf_or_crlf_and_the_last_may_have_neither() -> TestResult {
    let document = TomlDocument::parse(b"a = 1\r\n[t]\r\nb = \"x\"\r\n# c\r\nc = true")?;
    assert_eq!(
        rows(&document),
        [
            row("", "a", TomlValue::Integer(1)),
            row("t", "b", TomlValue::String("x".to_string())),
            row("t", "c", TomlValue::Boolean(true)),
        ]
    );
    Ok(())
}

#[test]
fn test_a_key_may_repeat_in_other_tables_and_name_its_table() -> TestResult {
    let document = TomlDocument::parse(b"sync = 1\n[a]\nsync = 2\na = 3\n[b]\nsync = 4\n")?;
    assert_eq!(
        rows(&document),
        [
            row("", "sync", TomlValue::Integer(1)),
            row("a", "sync", TomlValue::Integer(2)),
            row("a", "a", TomlValue::Integer(3)),
            row("b", "sync", TomlValue::Integer(4)),
        ]
    );
    Ok(())
}

#[test]
fn test_parsing_twice_gives_equal_documents() -> TestResult {
    let bytes = b"name = \"dev\"\n[storage]\npartition_count = 4\n";
    assert_eq!(TomlDocument::parse(bytes)?, TomlDocument::parse(bytes)?);
    Ok(())
}
