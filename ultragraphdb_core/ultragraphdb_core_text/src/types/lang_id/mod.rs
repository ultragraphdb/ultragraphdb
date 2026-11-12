/*
 * Copyright 2013 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, internal/language/lookup.go (type Language). Changed for ultragraphdb:
 * translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::tables::language_tables::{
    ALIAS_MAP, ALIAS_TYPES, LANG, LANG_NO_INDEX_OFFSET, SUPPRESS_SCRIPT,
};
use crate::types::script_id::ScriptId;
use crate::utils::tag_index::elem;

/// A base language identifier: an index into the language table, or, from `LANG_NO_INDEX_OFFSET` on, a
/// three-letter code encoded in base 26. Zero is `und`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct LangId(pub u16);

/// The kind of alias a language has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AliasType {
    Deprecated,
    Macro,
    Legacy,
    Unknown,
}

const BASE: u32 = (b'z' - b'a' + 1) as u32;

/// The base-26 value of a lower-case three-letter code.
pub(crate) fn str_to_int(s: &[u8]) -> u32 {
    s.iter()
        .fold(0, |v, &c| v * BASE + u32::from(c.wrapping_sub(b'a')))
}

/// The three letters of a base-26 value.
pub(crate) fn int_to_str(mut v: u32) -> [u8; 3] {
    let mut s = [0u8; 3];
    for slot in s.iter_mut().rev() {
        *slot = (v % BASE) as u8 + b'a';
        v /= BASE;
    }
    s
}

impl LangId {
    /// The BCP 47 representation: the two-letter code where one exists, else the three-letter one.
    pub(crate) fn as_string(self) -> String {
        if self.0 == 0 {
            return "und".to_string();
        }
        if self.0 >= LANG_NO_INDEX_OFFSET {
            let s = int_to_str(u32::from(self.0 - LANG_NO_INDEX_OFFSET));
            return String::from_utf8_lossy(&s).into_owned();
        }
        let [a, b, c, d] = elem(LANG, usize::from(self.0));
        if d == 0 {
            String::from_utf8_lossy(&[a, b, c]).into_owned()
        } else {
            String::from_utf8_lossy(&[a, b]).into_owned()
        }
    }

    /// The suggested replacement of the language and the kind of alias it is.
    pub(crate) fn canonicalize(self) -> (LangId, AliasType) {
        let k = ALIAS_MAP.partition_point(|alias| alias.from < self.0);
        if let Some(alias) = ALIAS_MAP.get(k)
            && alias.from == self.0
        {
            let typ = match ALIAS_TYPES.get(k) {
                Some(0) => AliasType::Deprecated,
                Some(1) => AliasType::Macro,
                Some(2) => AliasType::Legacy,
                _ => AliasType::Unknown,
            };
            return (LangId(alias.to), typ);
        }
        (self, AliasType::Unknown)
    }

    /// The script marked as Suppress-Script in the IANA registry, or zero.
    pub(crate) fn suppress_script(self) -> ScriptId {
        if self.0 < LANG_NO_INDEX_OFFSET {
            return ScriptId(
                SUPPRESS_SCRIPT
                    .get(usize::from(self.0))
                    .map_or(0, |&s| u16::from(s)),
            );
        }
        ScriptId(0)
    }
}
