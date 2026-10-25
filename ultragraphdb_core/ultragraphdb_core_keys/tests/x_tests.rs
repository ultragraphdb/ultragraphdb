/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, x/x_test.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_keys::{FACET_DELIMITER, STAR, predicate_lang, remove_duplicates, to_hex};

fn strings(s: &[&str]) -> Vec<String> {
    s.iter().map(|s| s.to_string()).collect()
}

#[test]
fn test_remove_duplicates() {
    let set = remove_duplicates(strings(&["a", "a", "a", "b", "b", "c", "c"]));
    assert_eq!(strings(&["a", "b", "c"]), set);
}

#[test]
fn test_remove_duplicates_without_duplicates() {
    let set = remove_duplicates(strings(&["a", "b", "c", "d"]));
    assert_eq!(strings(&["a", "b", "c", "d"]), set);
}

#[test]
fn test_remove_duplicates_sorts() {
    let set = remove_duplicates(strings(&["c", "a", "c", "B", "b"]));
    assert_eq!(strings(&["B", "a", "b", "c"]), set);
}

#[test]
fn test_remove_duplicates_edges() {
    assert!(remove_duplicates(vec![]).is_empty());
    assert_eq!(
        remove_duplicates(strings(&["", "", "a"])),
        strings(&["", "a"])
    );
    // Go's sort.Strings orders bytewise: "Z" (0x5a) < "a" < "é" (0xc3 0xa9) < "名".
    assert_eq!(
        remove_duplicates(strings(&["名", "é", "a", "Z", "é", "名"])),
        strings(&["Z", "a", "é", "名"])
    );
}

#[test]
fn test_to_hex() {
    assert_eq!(b"\"0x0\"".to_vec(), to_hex(0, false));
    assert_eq!(b"<0x0>".to_vec(), to_hex(0, true));
    assert_eq!(b"\"0xf\"".to_vec(), to_hex(15, false));
    assert_eq!(b"<0xf>".to_vec(), to_hex(15, true));
    assert_eq!(b"\"0x19\"".to_vec(), to_hex(25, false));
    assert_eq!(b"<0x19>".to_vec(), to_hex(25, true));
    assert_eq!(b"\"0xff\"".to_vec(), to_hex(255, false));
    assert_eq!(b"<0xff>".to_vec(), to_hex(255, true));
    assert_eq!(b"\"0xffffffffffffffff\"".to_vec(), to_hex(u64::MAX, false));
    assert_eq!(b"<0xffffffffffffffff>".to_vec(), to_hex(u64::MAX, true));
}

#[test]
fn test_predicate_lang() {
    let cases = [
        ("name@en", ("name", "en")),
        ("name", ("name", "")),
        ("name@", ("name", "")),
        ("@en", ("@en", "")),
        ("a@b@c", ("a@b", "c")),
        ("名前@ja", ("名前", "ja")),
        ("", ("", "")),
    ];
    for (input, expected) in cases {
        assert_eq!(predicate_lang(input), expected, "{input}");
    }
}

#[test]
fn test_constants() {
    assert_eq!(STAR, "_STAR_ALL");
    assert_eq!(FACET_DELIMITER, "|");
}
