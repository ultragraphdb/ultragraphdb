/*
 * Copyright 2015 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, internal/tag/tag.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! A sorted index of four-byte entries, searched by a prefix of up to four bytes.

use std::cmp::Ordering;

/// The entry at position `x`.
pub(crate) fn elem(s: &'static [u8], x: usize) -> &'static [u8] {
    &s[x * 4..x * 4 + 4]
}

/// The position of the first entry starting with `key`, if any.
pub(crate) fn index(s: &[u8], key: &[u8]) -> Option<usize> {
    let n = key.len();
    // The first entry with an equal or higher value than key.
    let count = s.len() / 4;
    let pos = partition_point(count, |i| cmp(&s[i * 4..i * 4 + n], key) == Ordering::Less);
    let i = pos * 4;
    if cmp(&s[i..i + key.len()], key) != Ordering::Equal {
        return None;
    }
    Some(pos)
}

/// The position after `x` if that entry also starts with `key`.
pub(crate) fn next(s: &[u8], key: &[u8], x: usize) -> Option<usize> {
    let x = x + 1;
    if x * 4 < s.len() && cmp(&s[x * 4..x * 4 + key.len()], key) == Ordering::Equal {
        return Some(x);
    }
    None
}

/// Go's `sort.Search`: the smallest `i` in `0..n` for which `pred(i)` is false (`pred` is "less than").
pub(crate) fn partition_point(n: usize, pred: impl Fn(usize) -> bool) -> usize {
    let (mut lo, mut hi) = (0, n);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if pred(mid) {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo
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
    for (i, slot) in b.iter_mut().enumerate() {
        let mut c = *slot;
        if form[i] <= b'Z' {
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
