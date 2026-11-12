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
use crate::utils::tag_index::{self, elem, fix_case};

/// The index of `key` in `idx`, after converting it to the case of `form`.
fn find_index(idx: &[u8], key: &mut [u8], form: &[u8]) -> Result<u16, LanguageError> {
    if !fix_case(form, key) {
        return Err(LanguageError::syntax());
    }
    tag_index::index(idx, key)
        .and_then(|i| u16::try_from(i).ok())
        .ok_or_else(|| LanguageError::unknown_subtag(key))
}

/// The identifier of the entry at `x` of a table, which holds fewer than 2^16 entries; a position past them names no
/// subtag of `s`.
fn table_id(x: usize, s: &[u8]) -> Result<u16, LanguageError> {
    u16::try_from(x).map_err(|_| LanguageError::unknown_subtag(s))
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
        return Ok(LangId(table_id(i, s)?));
    }
    Err(LanguageError::unknown_subtag(s))
}

fn get_lang_iso3(s: &mut [u8]) -> Result<LangId, LanguageError> {
    if !fix_case(b"und", s) {
        return Err(LanguageError::syntax());
    }
    // fix_case took three letters.
    let [c0, c1, c2] = <[u8; 3]>::try_from(&*s).map_err(|_| LanguageError::syntax())?;
    // First try to match canonical 3-letter entries.
    let mut i = tag_index::index(LANG, &[c0, c1]);
    while let Some(x) = i {
        let e = elem(LANG, x);
        if e[3] == 0 && e[2] == c2 {
            let id = table_id(x, s)?;
            // "und" is special and always translates to unspecified.
            if id == NON_CANONICAL_UND {
                return Ok(LangId(0));
            }
            return Ok(LangId(id));
        }
        i = tag_index::next(LANG, &[c0, c1], x);
    }
    if let Some(x) = tag_index::index(ALT_LANG_ISO3, s)
        && let Some(&id) = ALT_LANG_INDEX.get(usize::from(elem(ALT_LANG_ISO3, x)[3]))
    {
        return Ok(LangId(id));
    }
    // Three letters in base 26 are below 26^3, so the conversion succeeds.
    let id = u16::try_from(str_to_int(s)).map_err(|_| LanguageError::unknown_subtag(s))?;
    let n = usize::from(id);
    if LANG_NO_INDEX
        .get(n / 8)
        .is_some_and(|bits| bits & (1 << (n % 8)) != 0)
    {
        return Ok(LangId(id + LANG_NO_INDEX_OFFSET));
    }
    // Check for non-canonical uses of ISO3.
    let mut non_canonical = tag_index::index(LANG, &[c0]);
    while let Some(x) = non_canonical {
        let e = elem(LANG, x);
        if e[2] == c1 && e[3] == c2 {
            return Ok(LangId(table_id(x, s)?));
        }
        non_canonical = tag_index::next(LANG, &[c0], x);
    }
    Err(LanguageError::unknown_subtag(s))
}

/// The region of a two-letter or three-letter ISO code or a three-digit UN M.49 code.
pub(crate) fn get_region_id(s: &mut [u8]) -> Result<RegionId, LanguageError> {
    if s.len() == 3 {
        if s.first().is_some_and(|&c| c > b'9') {
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
    Ok(RegionId(i + ISO_REGION_OFFSET))
}

fn get_region_iso3(s: &mut [u8]) -> Result<RegionId, LanguageError> {
    if !fix_case(b"ZZZ", s) {
        return Err(LanguageError::syntax());
    }
    // fix_case took three letters.
    let [c0, c1, c2] = <[u8; 3]>::try_from(&*s).map_err(|_| LanguageError::syntax())?;
    let mut i = tag_index::index(REGION_ISO, &[c0]);
    while let Some(x) = i {
        let e = elem(REGION_ISO, x);
        if e[2] == c1 && e[3] == c2 {
            return Ok(RegionId(table_id(x, s)? + ISO_REGION_OFFSET));
        }
        i = tag_index::next(REGION_ISO, &[c0], x);
    }
    for (alt, id) in ALT_REGION_ISO3
        .as_chunks::<3>()
        .0
        .iter()
        .zip(ALT_REGION_IDS)
    {
        if *alt == [c0, c1, c2] {
            return Ok(RegionId(id));
        }
    }
    Err(LanguageError::unknown_subtag(s))
}

/// The region of a UN M.49 code. Go writes the code into a fresh buffer rather than the error, so the error's
/// subtag is empty.
pub(crate) fn get_region_m49(n: usize) -> Result<RegionId, LanguageError> {
    if let Ok(code) = u16::try_from(n)
        && 0 < code
        && code <= 999
    {
        const SEARCH_BITS: usize = 7;
        const REGION_BITS: u16 = 9;
        const REGION_MASK: u16 = (1 << REGION_BITS) - 1;
        let idx = n >> SEARCH_BITS;
        let (Some(&lo), Some(&hi)) = (M49_INDEX.get(idx), M49_INDEX.get(idx + 1)) else {
            return Err(LanguageError::unknown_subtag(&[]));
        };
        let (lo, hi) = (usize::from(lo), usize::from(hi));
        let buf = FROM_M49.get(lo..hi).unwrap_or_default();
        let val = code << REGION_BITS; // we rely on bits shifting out
        let i = buf.partition_point(|&entry| entry < val);
        // Go reads one past the bucket when no entry is greater; past the table's end that panics, which Parse
        // recovers as a syntax error. No code from 1 to 999 reaches past the table's end.
        if let Some(&r) = FROM_M49.get(lo + i)
            && r & !REGION_MASK == val
        {
            return Ok(RegionId(r & REGION_MASK));
        }
    }
    Err(LanguageError::unknown_subtag(&[]))
}

/// The script of a four-letter code.
pub(crate) fn get_script_id(s: &mut [u8]) -> Result<ScriptId, LanguageError> {
    let i = find_index(SCRIPT, s, b"Zzzz")?;
    Ok(ScriptId(i))
}
