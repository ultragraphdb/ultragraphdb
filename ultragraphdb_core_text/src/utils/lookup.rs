/*
 * Copyright 2013 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, internal/language/lookup.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Subtag lookups: a subtag's identifier from its text. Each lookup converts the case of its input in place, as
//! the parser relies on.

use crate::errors::language_error::LanguageError;
use crate::tables::language_tables::{
    ALT_LANG_INDEX, ALT_LANG_ISO3, ALT_REGION_IDS, ALT_REGION_ISO3, FROM_M49, ISO_REGION_OFFSET,
    LANG, LANG_NO_INDEX, LANG_NO_INDEX_OFFSET, M49_INDEX, NON_CANONICAL_UND, REGION_ISO, SCRIPT,
};
use crate::types::lang_id::{LangId, str_to_int};
use crate::types::region_id::RegionId;
use crate::types::script_id::ScriptId;
use crate::utils::tag_index::{self, elem, fix_case, partition_point};

/// The index of `key` in `idx`, after converting it to the case of `form`.
fn find_index(idx: &[u8], key: &mut [u8], form: &[u8]) -> Result<usize, LanguageError> {
    if !fix_case(form, key) {
        return Err(LanguageError::syntax());
    }
    tag_index::index(idx, key).ok_or_else(|| LanguageError::value(key))
}

/// The language of a two- or three-letter code; `und` is zero.
pub(crate) fn get_lang_id(s: &mut [u8]) -> Result<LangId, LanguageError> {
    if s.len() == 2 {
        return get_lang_iso2(s);
    }
    get_lang_iso3(s)
}

fn get_lang_iso2(s: &mut [u8]) -> Result<LangId, LanguageError> {
    if !fix_case(b"zz", s) {
        return Err(LanguageError::syntax());
    }
    if let Some(i) = tag_index::index(LANG, s)
        && elem(LANG, i)[3] != 0
    {
        return Ok(LangId(i as u16));
    }
    Err(LanguageError::value(s))
}

fn get_lang_iso3(s: &mut [u8]) -> Result<LangId, LanguageError> {
    if !fix_case(b"und", s) {
        return Err(LanguageError::syntax());
    }
    // First try to match canonical 3-letter entries.
    let mut i = tag_index::index(LANG, &s[..2]);
    while let Some(x) = i {
        let e = elem(LANG, x);
        if e[3] == 0 && e[2] == s[2] {
            // "und" is special and always translates to unspecified.
            if x as u16 == NON_CANONICAL_UND {
                return Ok(LangId(0));
            }
            return Ok(LangId(x as u16));
        }
        i = tag_index::next(LANG, &s[..2], x);
    }
    if let Some(x) = tag_index::index(ALT_LANG_ISO3, s) {
        return Ok(LangId(
            ALT_LANG_INDEX[usize::from(elem(ALT_LANG_ISO3, x)[3])],
        ));
    }
    let n = str_to_int(s) as usize;
    if LANG_NO_INDEX[n / 8] & (1 << (n % 8)) != 0 {
        return Ok(LangId(n as u16 + LANG_NO_INDEX_OFFSET));
    }
    // Check for non-canonical uses of ISO3.
    let mut i = tag_index::index(LANG, &s[..1]);
    while let Some(x) = i {
        let e = elem(LANG, x);
        if e[2] == s[1] && e[3] == s[2] {
            return Ok(LangId(x as u16));
        }
        i = tag_index::next(LANG, &s[..1], x);
    }
    Err(LanguageError::value(s))
}

/// The region of a two-letter or three-letter ISO code or a three-digit UN M.49 code.
pub(crate) fn get_region_id(s: &mut [u8]) -> Result<RegionId, LanguageError> {
    if s.len() == 3 {
        if s[0] > b'9' {
            return get_region_iso3(s);
        }
        if s.iter().all(u8::is_ascii_digit) {
            let i = s
                .iter()
                .fold(0usize, |v, &c| v * 10 + usize::from(c - b'0'));
            return get_region_m49(i);
        }
    }
    get_region_iso2(s)
}

fn get_region_iso2(s: &mut [u8]) -> Result<RegionId, LanguageError> {
    let i = find_index(REGION_ISO, s, b"ZZ")?;
    Ok(RegionId(i as u16 + ISO_REGION_OFFSET))
}

fn get_region_iso3(s: &mut [u8]) -> Result<RegionId, LanguageError> {
    if !fix_case(b"ZZZ", s) {
        return Err(LanguageError::syntax());
    }
    let mut i = tag_index::index(REGION_ISO, &s[..1]);
    while let Some(x) = i {
        let e = elem(REGION_ISO, x);
        if e[2] == s[1] && e[3] == s[2] {
            return Ok(RegionId(x as u16 + ISO_REGION_OFFSET));
        }
        i = tag_index::next(REGION_ISO, &s[..1], x);
    }
    for i in (0..ALT_REGION_ISO3.len()).step_by(3) {
        if ALT_REGION_ISO3[i..i + 3] == *s {
            return Ok(RegionId(ALT_REGION_IDS[i / 3]));
        }
    }
    Err(LanguageError::value(s))
}

/// The region of a UN M.49 code. Go writes the code into a fresh buffer rather than the error, so the error's
/// subtag is empty.
pub(crate) fn get_region_m49(n: usize) -> Result<RegionId, LanguageError> {
    if 0 < n && n <= 999 {
        const SEARCH_BITS: usize = 7;
        const REGION_BITS: u16 = 9;
        const REGION_MASK: u16 = (1 << REGION_BITS) - 1;
        let idx = n >> SEARCH_BITS;
        let lo = M49_INDEX[idx] as usize;
        let buf = &FROM_M49[lo..M49_INDEX[idx + 1] as usize];
        let val = (n as u16) << REGION_BITS; // we rely on bits shifting out
        let i = partition_point(buf.len(), |i| buf[i] < val);
        // Go reads one past the bucket when no entry is greater; past the table's end that panics, which Parse
        // recovers as a syntax error.
        let r = FROM_M49[lo + i];
        if r & !REGION_MASK == val {
            return Ok(RegionId(r & REGION_MASK));
        }
    }
    Err(LanguageError::value(&[]))
}

/// The script of a four-letter code.
pub(crate) fn get_script_id(s: &mut [u8]) -> Result<ScriptId, LanguageError> {
    let i = find_index(SCRIPT, s, b"Zzzz")?;
    Ok(ScriptId(i as u16))
}
