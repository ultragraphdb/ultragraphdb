/*
 * Copyright 2011 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, unicode/norm/normalize.go, unicode/norm/composition.go and
 * unicode/norm/forminfo.go (the parts the collation iterator calls), and from the Go standard library,
 * unicode/utf8/utf8.go (DecodeRune). Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! The normalization properties the collator reads: NFD combining classes and leading non-starters, the first
//! segment boundary, NFKD decompositions of single runes, and Hangul decomposition.

use crate::tables::norm_tables::{NFD_PROPERTIES, NFKD_DATA, NFKD_INDEX};

/// The maximum number of successive non-starters in a stream-safe segment.
const MAX_NON_STARTERS: u32 = 30;

/// The maximum size of a byte buffer holding one segment (`norm.MaxSegmentSize`).
pub(crate) const MAX_SEGMENT_SIZE: usize = 128;

/// The NFD properties of a rune: its combining class, of the first and the last rune of its decomposition, and
/// the number of leading non-starters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NfdProperties {
    pub lead_ccc: u8,
    pub trail_ccc: u8,
    pub n_lead: u8,
}

pub(crate) fn nfd_properties(r: char) -> NfdProperties {
    let r = r as u32;
    let i = NFD_PROPERTIES.partition_point(|&(_, hi, ..)| hi < r);
    match NFD_PROPERTIES.get(i) {
        Some(&(lo, _, lead_ccc, trail_ccc, n_lead)) if lo <= r => NfdProperties {
            lead_ccc,
            trail_ccc,
            n_lead,
        },
        _ => NfdProperties {
            lead_ccc: 0,
            trail_ccc: 0,
            n_lead: 0,
        },
    }
}

/// Go's `utf8.DecodeRune`: the first rune of `s` and its width in bytes; `(U+FFFD, 1)` when `s` starts with
/// invalid UTF-8, a sequence cut short included, and `(U+FFFD, 0)` when `s` is empty. Go and Rust agree on which
/// sequences are valid (no overlong encodings, no surrogates, nothing past U+10FFFF).
pub(crate) fn first_rune(s: &[u8]) -> (char, usize) {
    match s.utf8_chunks().next() {
        None => ('\u{fffd}', 0),
        Some(chunk) => chunk
            .valid()
            .chars()
            .next()
            .map_or(('\u{fffd}', 1), |c| (c, c.len_utf8())),
    }
}

/// Whether `s` holds too few bytes to complete the encoding its first byte starts, which norm's trie reports as
/// invalid properties. Any other invalid sequence has the properties of a starter without decomposition, as U+FFFD
/// has.
fn is_cut_short(s: &[u8]) -> bool {
    let need = match s[0] {
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        _ => 1,
    };
    s.len() < need
}

/// `norm.NFD.FirstBoundary`: the position of the first segment boundary in `s` after its leading continuation
/// bytes, a rune with no leading non-starters or the rune at which the stream-safe count overflows. `None` when there
/// is none before the end or a sequence is cut short (Go then returns -1 or the length, which its only caller treats
/// alike). An invalid sequence is a boundary, so its width never matters.
pub(crate) fn first_boundary(s: &[u8]) -> Option<usize> {
    let mut i = s.iter().position(|&b| b & 0xc0 != 0x80)?;
    let mut ss = 0u32;
    while i < s.len() {
        if is_cut_short(&s[i..]) {
            return None;
        }
        let (r, size) = first_rune(&s[i..]);
        let n = u32::from(nfd_properties(r).n_lead);
        ss += n;
        if ss > MAX_NON_STARTERS || n == 0 {
            return Some(i);
        }
        i += size;
    }
    None
}

/// The NFKD decomposition of a rune the collation tables decompose; `None` for any other rune.
pub(crate) fn nfkd_decomposition(r: char) -> Option<String> {
    let r = r as u32;
    let i = NFKD_INDEX.binary_search_by_key(&r, |&(c, ..)| c).ok()?;
    let (_, start, end) = NFKD_INDEX[i];
    Some(
        NFKD_DATA[usize::from(start)..usize::from(end)]
            .iter()
            .filter_map(|&c| char::from_u32(c))
            .collect(),
    )
}

const S_BASE: u32 = 0xAC00;
const L_BASE: u32 = 0x1100;
const V_BASE: u32 = 0x1161;
const T_BASE: u32 = 0x11A7;
const T_COUNT: u32 = 28;
const N_COUNT: u32 = 21 * T_COUNT;

/// The NFD decomposition of a precomposed Hangul syllable into its jamo, as UTF-8.
pub(crate) fn hangul_nfd(r: char) -> Vec<u8> {
    let s = r as u32 - S_BASE;
    let mut out = String::new();
    let jamo = [
        L_BASE + s / N_COUNT,
        V_BASE + (s % N_COUNT) / T_COUNT,
        T_BASE + s % T_COUNT,
    ];
    for (k, &j) in jamo.iter().enumerate() {
        if k == 2 && j == T_BASE {
            break;
        }
        out.extend(char::from_u32(j));
    }
    out.into_bytes()
}
