/*
 * Copyright (c) 2014 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/token/lowercase/lowercase.go, and Go's bytes.ToLower it falls back to.
 * Changed for ultragraphdb: translated to Rust over Go's unicode.ToLower.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_common_utils::{go_unicode, go_utf8};

fn push_rune(out: &mut Vec<u8>, r: u32) {
    let c = char::from_u32(r).unwrap_or(char::REPLACEMENT_CHARACTER);
    let mut buf = [0u8; 4];
    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
}

/// bleve's `toLowerDeferredCopy`: the term lowercased in place, a final `Σ` as `ς`. When a rune's lowercase form is
/// wider than the rune, the rest goes through Go's `bytes.ToLower`, which writes invalid bytes as U+FFFD and has
/// no final-sigma rule.
///
/// bleve moves only the runes it changes: once a rune has shrunk (`İ`, two bytes, lowercases to `i`, one), the
/// runes it leaves unchanged keep their old bytes in place and the term is cut to the written length, so the
/// bytes after the shrunk rune come out shifted and truncated. This port does the same, as the index holds those
/// terms.
///
/// `s` is the term's bytes in their buffer, lowercased in place as Go does; the result is `Ok(len)` when the term
/// is now `s[..len]` of that buffer, and `Err(copy)` when bleve copied it.
pub(crate) fn to_lower_deferred_copy(s: &mut [u8]) -> Result<usize, Vec<u8>> {
    let (mut i, mut j) = (0, 0);
    while i < s.len() {
        let (r, wid) = if s[i] < 0x80 {
            (u32::from(s[i]), 1)
        } else {
            go_utf8::decode_rune(&s[i..])
        };
        let mut l = go_unicode::to_lower(r);
        if l == r {
            i += wid;
            j += wid;
            continue;
        }
        if l == 0x03C3 && i + 2 == s.len() {
            l = 0x03C2;
        }
        let c = char::from_u32(l).unwrap_or(char::REPLACEMENT_CHARACTER);
        let lwid = c.len_utf8();
        if lwid > wid {
            let mut rv = s[..j].to_vec();
            rv.extend(bytes_to_lower(&s[i..]));
            return Err(rv);
        }
        let mut buf = [0u8; 4];
        s[j..j + lwid].copy_from_slice(c.encode_utf8(&mut buf).as_bytes());
        i += wid;
        j += lwid;
    }
    Ok(j)
}

/// Go's `bytes.ToLower`.
fn bytes_to_lower(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let (r, wid) = go_utf8::decode_rune(&s[i..]);
        let r = if r == go_utf8::RUNE_ERROR && wid == 1 {
            go_utf8::RUNE_ERROR
        } else {
            r
        };
        push_rune(&mut out, go_unicode::to_lower(r));
        i += wid;
    }
    out
}
