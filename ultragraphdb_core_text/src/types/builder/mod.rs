/*
 * Copyright 2018 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, internal/language/compose.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::tables::language_tables::VARIANT_INDEX;
use crate::types::full_tag::{FullTag, MAX_CORE_SIZE};
use crate::types::scanner::Scanner;
use crate::utils::parse::parse_scanner;

/// Constructs a tag from individual components.
#[derive(Debug, Clone, Default)]
pub(crate) struct Builder {
    pub tag: FullTag,
    /// The x extension.
    private: String,
    variants: Vec<String>,
    extensions: Vec<String>,
}

impl Builder {
    /// The tag of the current settings.
    pub(crate) fn make(&mut self) -> FullTag {
        let mut t = self.tag.clone();
        if !self.extensions.is_empty() || !self.variants.is_empty() {
            self.variants.sort_by_key(|v| variant_order(v));
            self.extensions.sort();
            if !self.private.is_empty() {
                self.extensions.push(self.private.clone());
            }
            let n = MAX_CORE_SIZE + token_len(&self.variants) + token_len(&self.extensions);
            let mut buf = Vec::with_capacity(n);
            buf.extend_from_slice(&t.gen_core_bytes());
            t.p_variant = buf.len() as u8;
            append_tokens(&mut buf, &self.variants);
            t.p_ext = buf.len() as u16;
            append_tokens(&mut buf, &self.extensions);
            t.str = String::from_utf8_lossy(&buf).into_owned();
            // The string is parsed again, as remaking it is not always needed but tricky to decide.
            let mut scan = Scanner::new(buf);
            return match parse_scanner(&mut scan, b"") {
                Ok(t) | Err((t, _)) => t,
            };
        } else if !self.private.is_empty() {
            t.str = self.private.clone();
            t.remake_string();
        }
        t
    }

    /// Copies all settings from `t`, discarding any set before.
    pub(crate) fn set_tag(&mut self, t: &FullTag) {
        self.tag.lang_id = t.lang_id;
        self.tag.region_id = t.region_id;
        self.tag.script_id = t.script_id;
        self.variants.clear();
        let variants = t.variants();
        if !variants.is_empty() {
            self.variants
                .extend(variants[1..].split('-').map(str::to_string));
        }
        self.extensions.clear();
        self.private.clear();
        for e in t.extensions() {
            self.add_ext(&e);
        }
    }

    /// Adds the extension `e`. An extension already present is kept, except that a `-u` extension gains the
    /// key-type pairs of `e`.
    pub(crate) fn add_ext(&mut self, e: &str) {
        if e.as_bytes()[0] == b'x' {
            if self.private.is_empty() {
                self.private = e.to_string();
            }
            return;
        }
        for s in self.extensions.iter_mut() {
            if s.as_bytes()[0] == e.as_bytes()[0] {
                if e.as_bytes()[0] == b'u' {
                    s.push_str(&e[1..]);
                }
                return;
            }
        }
        self.extensions.push(e.to_string());
    }

    /// Sets the extension `e`, overwriting one of the same type, except that a `-u` extension's key-type pairs
    /// are set individually.
    pub(crate) fn set_ext(&mut self, e: &str) {
        if e.as_bytes()[0] == b'x' {
            self.private = e.to_string();
            return;
        }
        for s in self.extensions.iter_mut() {
            if s.as_bytes()[0] == e.as_bytes()[0] {
                if e.as_bytes()[0] == b'u' {
                    *s = format!("{}{}", e, &s[1..]);
                } else {
                    *s = e.to_string();
                }
                return;
            }
        }
        self.extensions.push(e.to_string());
    }

    /// Adds the variants, skipping empty ones.
    pub(crate) fn add_variant(&mut self, v: &str) {
        if !v.is_empty() {
            self.variants.push(v.to_string());
        }
    }

    /// Removes all extensions, the x extension included.
    pub(crate) fn clear_extensions(&mut self) {
        self.private.clear();
        self.extensions.clear();
    }
}

/// The sort key of a variant: its identifier, zero for an unknown one.
fn variant_order(v: &str) -> u8 {
    VARIANT_INDEX
        .binary_search_by(|(k, _)| (*k).cmp(v))
        .map_or(0, |i| VARIANT_INDEX[i].1)
}

fn token_len(tokens: &[String]) -> usize {
    tokens.iter().map(|t| t.len() + 1).sum()
}

fn append_tokens(b: &mut Vec<u8>, tokens: &[String]) {
    for t in tokens {
        b.push(b'-');
        b.extend_from_slice(t.as_bytes());
    }
}
