/*
 * Copyright 2015 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, internal/tag/tag.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! A sorted index of four-byte entries, searched by a prefix of up to four bytes.

use std::cmp::Ordering;

/// The entry at position `x`; four zero bytes past the last.
pub(crate) fn elem(s: &'static [u8], x: usize) -> [u8; 4] {
    s.as_chunks::<4>().0.get(x).copied().unwrap_or_default()
}

/// The first `key.len()` bytes of `entry`, a key of at most four bytes being compared with an entry's prefix.
fn prefix<'a>(entry: &'a [u8; 4], key: &[u8]) -> &'a [u8] {
    entry.get(..key.len()).unwrap_or(entry)
}

/// The position of the first entry starting with `key`, if any.
pub(crate) fn index(s: &[u8], key: &[u8]) -> Option<usize> {
    let entries = s.as_chunks::<4>().0;
    // The first entry with an equal or higher value than key.
    let pos = entries.partition_point(|e| cmp(prefix(e, key), key) == Ordering::Less);
    let entry = entries.get(pos)?;
    if cmp(prefix(entry, key), key) != Ordering::Equal {
        return None;
    }
    Some(pos)
}

/// The position after `x` if that entry also starts with `key`.
pub(crate) fn next(s: &[u8], key: &[u8], x: usize) -> Option<usize> {
    let x = x + 1;
    let entry = s.as_chunks::<4>().0.get(x)?;
    (cmp(prefix(entry, key), key) == Ordering::Equal).then_some(x)
}

/// Byte-wise comparison, a shorter prefix first.
pub(crate) fn cmp(a: &[u8], b: &[u8]) -> Ordering {
    a.cmp(b)
}

/// Converts the case of `b` to that of `form` (upper case where `form` has a letter up to `Z`), reporting whether
/// `b` has `form`'s length and only letters. Bytes are converted in place up to the first failure.
pub(crate) fn fix_case(form: &[u8], b: &mut [u8]) -> bool {
    if form.len() != b.len() {
        return false;
    }
    for (slot, &f) in b.iter_mut().zip(form) {
        let mut c = *slot;
        if f <= b'Z' {
            if c >= b'a' {
                c = c.wrapping_sub(b'z' - b'Z');
            }
            if !c.is_ascii_uppercase() {
                return false;
            }
        } else {
            if c <= b'Z' {
                c = c.wrapping_add(b'z' - b'Z');
            }
            if !c.is_ascii_lowercase() {
                return false;
            }
        }
        *slot = c;
    }
    true
}
