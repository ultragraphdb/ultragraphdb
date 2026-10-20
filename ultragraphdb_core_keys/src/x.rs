/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, x/x.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// Equivalent to using `*` in a mutation.
pub const STAR: &str = "_STAR_ALL";

/// The symbol used to distinguish predicate names from facets.
pub const FACET_DELIMITER: &str = "|";

/// Sorts the strings bytewise and removes duplicates.
pub fn remove_duplicates(mut s: Vec<String>) -> Vec<String> {
    s.sort();
    s.dedup();
    s
}

/// The predicate (or facet) name and its language tag, split at the last `@`; the tag is empty when there is no
/// `@` past the first byte.
pub fn predicate_lang(s: &str) -> (&str, &str) {
    match s.rfind('@') {
        Some(i) if i > 0 => (&s[..i], &s[i + 1..]),
        _ => (s, ""),
    }
}

/// The uid in hexadecimal, `0x…`, delimited by `<` `>` when `rdf` is set and by quotes, as JSON requires,
/// otherwise.
pub fn to_hex(i: u64, rdf: bool) -> Vec<u8> {
    let (open, close) = if rdf { ('<', '>') } else { ('"', '"') };
    format!("{open}0x{i:x}{close}").into_bytes()
}
