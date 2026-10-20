/*
 * Copyright 2013 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, internal/language/parse.go and the grandfathered tags of
 * internal/language/lookup.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! The BCP 47 parser of `internal/language`.

use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::errors::language_error::{LanguageError, LanguageErrorEnum};
use crate::tables::language_tables::{
    LANG_AMI, LANG_BNN, LANG_CMN, LANG_HAK, LANG_HSN, LANG_JBO, LANG_LB, LANG_NAN, LANG_NB,
    LANG_NN, LANG_NV, LANG_PWN, LANG_SFB, LANG_SGG, LANG_TAO, LANG_TAY, LANG_TLH, LANG_TSU,
    LANG_VGT, VARIANT_INDEX,
};
use crate::types::full_tag::FullTag;
use crate::types::lang_id::LangId;
use crate::types::scanner::Scanner;
use crate::utils::lookup::{get_lang_id, get_region_id, get_script_id};

const MAX_ALT_TAG_LEN: usize = "en-US-POSIX".len();

/// Legacy and grandfathered tags with their base language, or, when negative, the index of a replacement tag
/// in `ALT_TAGS`.
static GRANDFATHERED: [(&[u8], i32); 28] = [
    (b"art-lojban", LANG_JBO as i32),
    (b"i-ami", LANG_AMI as i32),
    (b"i-bnn", LANG_BNN as i32),
    (b"i-hak", LANG_HAK as i32),
    (b"i-klingon", LANG_TLH as i32),
    (b"i-lux", LANG_LB as i32),
    (b"i-navajo", LANG_NV as i32),
    (b"i-pwn", LANG_PWN as i32),
    (b"i-tao", LANG_TAO as i32),
    (b"i-tay", LANG_TAY as i32),
    (b"i-tsu", LANG_TSU as i32),
    (b"no-bok", LANG_NB as i32),
    (b"no-nyn", LANG_NN as i32),
    (b"sgn-be-fr", LANG_SFB as i32),
    (b"sgn-be-nl", LANG_VGT as i32),
    (b"sgn-ch-de", LANG_SGG as i32),
    (b"zh-guoyu", LANG_CMN as i32),
    (b"zh-hakka", LANG_HAK as i32),
    (b"zh-min-nan", LANG_NAN as i32),
    (b"zh-xiang", LANG_HSN as i32),
    // Grandfathered tags with no modern replacement.
    (b"cel-gaulish", -1),
    (b"en-gb-oed", -2),
    (b"i-default", -3),
    (b"i-enochian", -4),
    (b"i-mingo", -5),
    (b"zh-min", -6),
    // CLDR-specific tag.
    (b"root", 0),
    (b"en-us-posix", -7),
];

const ALT_TAG_INDEX: [usize; 8] = [0, 17, 31, 45, 61, 74, 86, 102];

const ALT_TAGS: &str = "xtg-x-cel-gaulishen-GB-oxendicten-x-i-defaultund-x-i-enochiansee-x-i-mingonan-x-zh-minen-US-u-va-posix";

fn grandfathered(s: &[u8; MAX_ALT_TAG_LEN]) -> Option<FullTag> {
    let (_, v) = GRANDFATHERED.iter().find(|(k, _)| {
        let mut key = [0u8; MAX_ALT_TAG_LEN];
        key[..k.len()].copy_from_slice(k);
        &key == s
    })?;
    if *v < 0 {
        let i = (-*v) as usize;
        return Some(make(&ALT_TAGS[ALT_TAG_INDEX[i - 1]..ALT_TAG_INDEX[i]]));
    }
    Some(FullTag {
        lang_id: LangId(*v as u16),
        ..FullTag::default()
    })
}

/// Go's `Make`: `parse` without the error.
pub(crate) fn make(s: &str) -> FullTag {
    match parse(s) {
        Ok(t) | Err((t, _)) => t,
    }
}

/// Parses a BCP 47 string. On failure it returns the part of the tag that could be parsed with the error; an
/// unknown subtag is stripped and reported as a value error. Go recovers from a panic in the parser as `und`
/// with a syntax error; so does this port.
pub(crate) fn parse(s: &str) -> Result<FullTag, (FullTag, LanguageError)> {
    if s.is_empty() {
        return Err((FullTag::default(), LanguageError::syntax()));
    }
    let result = catch_unwind(AssertUnwindSafe(|| parse_unchecked(s)));
    result.unwrap_or_else(|_| Err((FullTag::default(), LanguageError::syntax())))
}

fn parse_unchecked(s: &str) -> Result<FullTag, (FullTag, LanguageError)> {
    if s.len() <= MAX_ALT_TAG_LEN {
        let mut b = [0u8; MAX_ALT_TAG_LEN];
        for (i, c) in s.char_indices() {
            // Generating invalid UTF-8 is okay as it won't match.
            let c = match c {
                'A'..='Z' => c.to_ascii_lowercase(),
                '_' => '-',
                _ => c,
            };
            b[i] = c as u32 as u8;
        }
        if let Some(t) = grandfathered(&b) {
            return Ok(t);
        }
    }
    let mut scan = Scanner::new(s.as_bytes().to_vec());
    parse_scanner(&mut scan, s.as_bytes())
}

/// Parses the tokens of `scan`; `s` is the original input, kept as the tag's string when it is already in
/// canonical form.
pub(crate) fn parse_scanner(
    scan: &mut Scanner,
    s: &[u8],
) -> Result<FullTag, (FullTag, LanguageError)> {
    let mut t = FullTag::default();
    let n = scan.token_len();
    let end = if n <= 1 {
        let len = scan.len();
        scan.lower_case(0, len);
        if n == 0 || scan.token()[0] != b'x' {
            return Err((t, LanguageError::syntax()));
        }
        parse_extensions(scan)
    } else if n >= 4 {
        return Err((FullTag::default(), LanguageError::syntax()));
    } else {
        // the usual case
        let (tag, mut e) = parse_tag(scan, true);
        t = tag;
        if scan.token_len() == 1 {
            t.p_ext = e as u16;
            e = parse_extensions(scan);
        } else if e < scan.len() {
            scan.set_error(Some(LanguageError::syntax()));
            scan.truncate(e);
        }
        e
    };
    if usize::from(t.p_variant) < scan.len() {
        let s = if end < s.len() { &s[..end] } else { s };
        if !s.is_empty() && s == scan.b() {
            t.str = String::from_utf8_lossy(s).into_owned();
        } else {
            t.str = String::from_utf8_lossy(scan.b()).into_owned();
        }
    } else {
        t.p_variant = 0;
        t.p_ext = 0;
    }
    match scan.err.clone() {
        None => Ok(t),
        Some(err) => Err((t, err)),
    }
}

/// Parses language, script, region and variants. Returns the tag and the end position in the input that was
/// parsed. If `do_norm` is set, `<lang>-<extlang>` is normalized to `<extlang>`.
pub(crate) fn parse_tag(scan: &mut Scanner, do_norm: bool) -> (FullTag, usize) {
    let mut t = FullTag::default();
    let r = get_lang_id(scan.token_mut());
    t.lang_id = r.as_ref().copied().unwrap_or_default();
    scan.set_error(r.err());
    scan.replace(t.lang_id.as_string().as_bytes());
    let lang_start = scan.start;
    let mut end = scan.scan();
    while scan.token_len() == 3 && is_alpha(scan.token()[0]) {
        // <lang>-<extlang> tags are equivalent to a tag of the form <extlang>.
        if do_norm {
            let r = get_lang_id(scan.token_mut());
            let lang = r.as_ref().copied().unwrap_or_default();
            if lang.0 != 0 {
                t.lang_id = lang;
                let lang_str = lang.as_string();
                let n = lang_str.len();
                let b = scan.b_mut();
                b[lang_start..lang_start + n].copy_from_slice(lang_str.as_bytes());
                b[lang_start + n] = b'-';
                scan.start = lang_start + n + 1;
            }
            scan.gobble(r.err());
        }
        end = scan.scan();
    }
    if scan.token_len() == 4 && is_alpha(scan.token()[0]) {
        let r = get_script_id(scan.token_mut());
        t.script_id = r.as_ref().copied().unwrap_or_default();
        if t.script_id.0 == 0 {
            scan.gobble(r.err());
        }
        end = scan.scan();
    }
    let n = scan.token_len();
    if (2..=3).contains(&n) {
        let r = get_region_id(scan.token_mut());
        t.region_id = r.as_ref().copied().unwrap_or_default();
        if t.region_id.0 == 0 {
            scan.gobble(r.err());
        } else {
            scan.replace(t.region_id.as_string().as_bytes());
        }
        end = scan.scan();
    }
    let (start, len) = (scan.start, scan.len());
    scan.lower_case(start, len);
    t.p_variant = end as u8;
    end = parse_variants(scan, end);
    t.p_ext = end as u16;
    (t, end)
}

/// Whether a letter or digit byte is a letter.
fn is_alpha(b: u8) -> bool {
    b > b'9'
}

fn variant_id(v: &[u8]) -> Option<u8> {
    VARIANT_INDEX
        .binary_search_by(|(k, _)| k.as_bytes().cmp(v))
        .ok()
        .map(|i| VARIANT_INDEX[i].1)
}

/// Scans tokens as long as each is a valid variant; duplicate variants are removed.
fn parse_variants(scan: &mut Scanner, mut end: usize) -> usize {
    let start = scan.start;
    let mut var_id: Vec<u8> = Vec::new();
    let mut variant: Vec<Vec<u8>> = Vec::new();
    let mut last: i32 = -1;
    let mut need_sort = false;
    while scan.token_len() >= 4 {
        let Some(v) = variant_id(scan.token()) else {
            // unknown variant
            let err = LanguageError::value(scan.token());
            scan.gobble(Some(err));
            scan.scan();
            continue;
        };
        var_id.push(v);
        variant.push(scan.token().to_vec());
        if !need_sort {
            if last < i32::from(v) {
                last = i32::from(v);
            } else {
                need_sort = true;
                // There is no legal combinations of more than 7 variants.
                const MAX_VARIANTS: usize = 8;
                if var_id.len() > MAX_VARIANTS {
                    break;
                }
            }
        }
        end = scan.end;
        scan.scan();
    }
    if need_sort {
        let mut pairs: Vec<(u8, Vec<u8>)> = var_id.into_iter().zip(variant).collect();
        pairs.sort_by_key(|(id, _)| *id);
        pairs.dedup_by_key(|(id, _)| *id);
        let s = pairs
            .into_iter()
            .map(|(_, v)| v)
            .collect::<Vec<_>>()
            .join(&b'-');
        if s.is_empty() {
            end = start - 1;
        } else {
            scan.resize_range(start, end, s.len());
            let st = scan.start;
            scan.copy_at(st, &s);
            end = scan.end;
        }
    }
    end
}

/// Parses and normalizes the extensions in the buffer. Returns the last position of the buffer that is part of
/// any extension and trims the buffer accordingly.
pub(crate) fn parse_extensions(scan: &mut Scanner) -> usize {
    let start = scan.start;
    let mut exts: Vec<Vec<u8>> = Vec::new();
    let mut private: Vec<u8> = Vec::new();
    let mut end = scan.end;
    while scan.token_len() == 1 {
        let ext_start = scan.start;
        let ext = scan.token()[0];
        end = parse_extension(scan);
        let extension = scan.b()[ext_start..end].to_vec();
        if extension.len() < 3 || (ext != b'x' && extension.len() < 4) {
            scan.set_error(Some(LanguageError::syntax()));
            end = ext_start;
            continue;
        } else if start == ext_start && (ext == b'x' || scan.start == scan.len()) {
            scan.truncate(end);
            return end;
        } else if ext == b'x' {
            private = extension;
            break;
        }
        exts.push(extension);
    }
    exts.sort_by_key(|e| e[0]);
    if !private.is_empty() {
        exts.push(private);
    }
    scan.truncate(start);
    if !exts.is_empty() {
        let joined = exts.join(&b'-');
        scan.append(&joined);
    } else if start > 0 {
        // Strip trailing '-'.
        scan.truncate(start - 1);
    }
    end
}

/// Parses a single extension and returns the position of its end.
fn parse_extension(scan: &mut Scanner) -> usize {
    let (start, mut end) = (scan.start, scan.end);
    match scan.token()[0] {
        b'u' => {
            let attr_start = end;
            scan.scan();
            let mut last: Vec<u8> = Vec::new();
            while scan.token_len() > 2 {
                if scan.token() >= last.as_slice() {
                    // Attributes are unsorted. Start over from scratch.
                    let p = attr_start + 1;
                    scan.next = p;
                    let mut attrs: Vec<Vec<u8>> = Vec::new();
                    scan.scan();
                    while scan.token_len() > 2 {
                        attrs.push(scan.token().to_vec());
                        end = scan.end;
                        scan.scan();
                    }
                    attrs.sort_by(|a, b| a[..3].cmp(&b[..3]));
                    scan.copy_at(p, &attrs.join(&b'-'));
                    break;
                }
                last = scan.token().to_vec();
                end = scan.end;
                scan.scan();
            }
            // Scan key-type sequences. A key is of length 2 and may be followed by 0 or more "type" subtags from 3
            // to the maximum of 8 letters.
            let attr_end = end;
            let mut last: Vec<u8> = Vec::new();
            while scan.token_len() == 2 {
                let key = scan.token().to_vec();
                end = scan.end;
                scan.scan();
                while end < scan.end && scan.token_len() > 2 {
                    end = scan.end;
                    scan.scan();
                }
                if key.as_slice() <= last.as_slice() || scan.err.is_some() {
                    // We have an invalid key or the keys are not sorted. Start scanning keys from scratch and
                    // reorder.
                    let p = attr_end + 1;
                    scan.next = p;
                    let mut keys: Vec<Vec<u8>> = Vec::new();
                    scan.scan();
                    while scan.token_len() == 2 {
                        let key_start = scan.start;
                        end = scan.end;
                        scan.scan();
                        while end < scan.end && scan.token_len() > 2 {
                            end = scan.end;
                            scan.scan();
                        }
                        keys.push(scan.b()[key_start..end].to_vec());
                    }
                    keys.sort_by(|a, b| a[..2].cmp(&b[..2]));
                    if !keys.is_empty() {
                        let mut k = 0;
                        for i in 1..keys.len() {
                            if keys[k][..2] != keys[i][..2] {
                                k += 1;
                                keys[k] = keys[i].clone();
                            } else if keys[k] != keys[i] {
                                scan.set_error(Some(LanguageError::new(
                                    LanguageErrorEnum::DuplicateKey,
                                )));
                            }
                        }
                        keys.truncate(k + 1);
                    }
                    let reordered = keys.join(&b'-');
                    let e = p + reordered.len();
                    if e < end {
                        scan.delete_range(e, end);
                        end = e;
                    }
                    scan.copy_at(p, &reordered);
                    break;
                }
                last = key;
            }
        }
        b't' => {
            scan.scan();
            let n = scan.token_len();
            if (2..=3).contains(&n) && is_alpha(scan.token()[1]) {
                let (_, e) = parse_tag(scan, false);
                end = e;
                scan.lower_case(start, end);
            }
            while scan.token_len() == 2 && !is_alpha(scan.token()[1]) {
                end = scan.accept_min_size(3);
            }
        }
        b'x' => {
            end = scan.accept_min_size(1);
        }
        _ => {
            end = scan.accept_min_size(2);
        }
    }
    end
}

/// The end position and the text of the extension at `p`.
pub(crate) fn get_extension(s: &[u8], mut p: usize) -> (usize, &[u8]) {
    if s[p] == b'-' {
        p += 1;
    }
    if s[p] == b'x' {
        return (s.len(), &s[p..]);
    }
    let end = next_extension(s, p);
    (end, &s[p..end])
}

/// The position of the next extension, the next `-<char>-` from `p`.
pub(crate) fn next_extension(s: &[u8], mut p: usize) -> usize {
    let n = s.len().saturating_sub(3);
    while p < n {
        if s[p] == b'-' {
            if s[p + 2] == b'-' {
                return p;
            }
            p += 3;
        } else {
            p += 1;
        }
    }
    s.len()
}
