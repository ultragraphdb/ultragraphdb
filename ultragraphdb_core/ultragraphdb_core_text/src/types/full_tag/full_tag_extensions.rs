/*
 * Copyright 2013 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, internal/language/language.go (Tag's extension methods). Changed for
 * ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use super::{FullTag, MAX_CORE_SIZE};
use crate::errors::language_error::{LanguageError, LanguageErrorEnum};
use crate::types::scanner::Scanner;
use crate::utils::parse::{get_extension, next_extension, parse_extensions};

impl FullTag {
    /// The extension of type `x`, if the tag has one.
    pub(crate) fn extension(&self, x: u8) -> Option<String> {
        let s = self.str.as_bytes();
        let mut i = usize::from(self.p_ext);
        while i + 1 < s.len() {
            let (end, ext) = get_extension(s, i);
            i = end;
            if ext[0] == x {
                return Some(String::from_utf8_lossy(ext).into_owned());
            }
        }
        None
    }

    /// All extensions of the tag.
    pub(crate) fn extensions(&self) -> Vec<String> {
        let s = self.str.as_bytes();
        let mut e = Vec::new();
        let mut i = usize::from(self.p_ext);
        while i + 1 < s.len() {
            let (end, ext) = get_extension(s, i);
            i = end;
            e.push(String::from_utf8_lossy(ext).into_owned());
        }
        e
    }

    /// The first type of the `-u` extension key `key`, or the empty string.
    pub(crate) fn type_for_key(&self, key: &str) -> String {
        let (_, start, end, _) = self.find_type_for_key(key);
        if end != start {
            let s = &self.str[start..end];
            let s = match s.find('-') {
                Some(p) => &s[..p],
                None => s,
            };
            return s.to_string();
        }
        String::new()
    }

    /// A tag with the `-u` extension key `key` set to `value`; an empty value removes the key.
    pub(crate) fn set_type_for_key(
        &self,
        key: &str,
        value: &str,
    ) -> Result<FullTag, (FullTag, LanguageError)> {
        let mut t = self.clone();
        if t.is_private_use() {
            return Err((t, LanguageError::new(LanguageErrorEnum::PrivateUse)));
        }
        if key.len() != 2 {
            return Err((t, LanguageError::new(LanguageErrorEnum::InvalidArguments)));
        }

        // Remove the setting if value is "".
        if value.is_empty() {
            let (mut start, sep, end, _) = t.find_type_for_key(key);
            if start != sep {
                let s = t.str.as_bytes();
                // Remove a possible empty extension.
                if s[start - 2] != b'-' {
                    // has previous elements.
                } else if end == s.len() || end + 2 < s.len() && s[end + 2] == b'-' {
                    // end of string or end of extension
                    start -= 2;
                }
                if start == usize::from(t.p_variant) && end == t.str.len() {
                    t.str = String::new();
                    t.p_variant = 0;
                    t.p_ext = 0;
                } else {
                    t.str = format!("{}{}", &t.str[..start], &t.str[end..]);
                }
            }
            return Ok(t);
        }

        if value.len() < 3 || value.len() > 8 {
            return Err((t, LanguageError::new(LanguageErrorEnum::InvalidArguments)));
        }

        // Generate the tag string if needed.
        let mut buf: Vec<u8> = Vec::with_capacity(MAX_CORE_SIZE + 14);
        if t.str.is_empty() {
            buf = t.gen_core_bytes();
            buf.push(b'-');
        }
        let u_start = buf.len();

        // Create new key-type pair and parse it to verify.
        let mut b = Vec::with_capacity(5 + value.len());
        b.extend_from_slice(b"u-");
        b.extend_from_slice(key.as_bytes());
        b.push(b'-');
        b.extend_from_slice(value.as_bytes());
        let mut scan = Scanner::new(b.clone());
        // The scanner converts '_' to '-' in Go's buffer itself.
        for c in b.iter_mut() {
            if *c == b'_' {
                *c = b'-';
            }
        }
        parse_extensions(&mut scan);
        if let Some(err) = scan.err.clone() {
            return Err((t, err));
        }

        // Assemble the replacement string.
        if t.str.is_empty() {
            t.p_variant = (u_start - 1) as u8;
            t.p_ext = (u_start - 1) as u16;
            buf.extend_from_slice(&b);
            t.str = String::from_utf8_lossy(&buf).into_owned();
        } else {
            let s = t.str.clone();
            let (start, sep, end, has_ext) = t.find_type_for_key(key);
            if start == sep {
                let b = if has_ext { &b[2..] } else { &b[..] };
                t.str = format!("{}-{}{}", &s[..sep], String::from_utf8_lossy(b), &s[end..]);
            } else {
                t.str = format!("{}-{}{}", &s[..start + 3], value, &s[end..]);
            }
        }
        Ok(t)
    }

    /// The start and end position of the type of `key`, or the point at which to insert the key-type pair if the
    /// key is absent, and whether a `-u` extension is present.
    pub(crate) fn find_type_for_key(&self, key: &str) -> (usize, usize, usize, bool) {
        let mut p = usize::from(self.p_ext);
        let s = self.str.as_bytes();
        if key.len() != 2 || p == s.len() || p == 0 {
            return (p, p, p, false);
        }
        let key = key.as_bytes();

        // Find the correct extension.
        p += 1;
        while s[p] != b'u' {
            if s[p] > b'u' {
                p -= 1;
                return (p, p, p, false);
            }
            p = next_extension(s, p);
            if p == s.len() {
                return (s.len(), s.len(), s.len(), false);
            }
            p += 1;
        }
        // Proceed to the hyphen following the extension name.
        p += 1;

        // The key currently being processed.
        let mut cur_key: &[u8] = b"";
        let (mut start, mut sep) = (0, 0);

        // Iterate over keys until we get the end of a section.
        loop {
            let end = p;
            p += 1;
            while p < s.len() && s[p] != b'-' {
                p += 1;
            }
            let n = p - end - 1;
            if n <= 2 && cur_key == key {
                if sep < end {
                    sep += 1;
                }
                return (start, sep, end, true);
            }
            match n {
                // invalid string, next extension
                0 | 1 => return (end, end, end, true),
                2 => {
                    // next key
                    cur_key = &s[end + 1..p];
                    if cur_key > key {
                        return (end, end, end, true);
                    }
                    start = end;
                    sep = p;
                }
                _ => {}
            }
        }
    }
}
