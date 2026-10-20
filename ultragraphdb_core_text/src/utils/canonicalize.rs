/*
 * Copyright 2013 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, language/language.go (CanonType and canonicalize). Changed for
 * ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::tables::language_tables::{
    LANG_MO, LANG_NB, LANG_NO, LANG_SH, REGION_MD, SCRIPT_LATN, SCRIPT_QAAI, SCRIPT_ZINH,
};
use crate::types::full_tag::FullTag;
use crate::types::lang_id::{AliasType, LangId};
use crate::types::region_id::RegionId;
use crate::types::script_id::ScriptId;

/// The canonicalizations to apply, as Go's `CanonType` bit set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CanonType(u32);

impl CanonType {
    /// Replace deprecated base languages with their preferred replacements.
    pub(crate) const DEPRECATED_BASE: CanonType = CanonType(1);
    /// Replace deprecated scripts with their preferred replacements.
    pub(crate) const DEPRECATED_SCRIPT: CanonType = CanonType(1 << 1);
    /// Replace deprecated regions with their preferred replacements.
    pub(crate) const DEPRECATED_REGION: CanonType = CanonType(1 << 2);
    /// Remove redundant scripts.
    pub(crate) const SUPPRESS_SCRIPT: CanonType = CanonType(1 << 3);
    /// Normalize legacy encodings: legacy languages defined in CLDR and bibliographic ISO 639 codes.
    pub(crate) const LEGACY: CanonType = CanonType(1 << 4);
    /// Map the dominant language of a macro language group to the macro language subtag.
    pub(crate) const MACRO: CanonType = CanonType(1 << 5);
    /// Full compatibility with CLDR.
    pub(crate) const CLDR: CanonType = CanonType(1 << 6);
    /// No canonicalization.
    pub(crate) const RAW: CanonType = CanonType(0);
    /// All deprecated tags replaced.
    pub(crate) const DEPRECATED: CanonType = CanonType(
        CanonType::DEPRECATED_BASE.0
            | CanonType::DEPRECATED_SCRIPT.0
            | CanonType::DEPRECATED_REGION.0,
    );
    /// All canonicalizations recommended by BCP 47.
    pub(crate) const BCP47: CanonType =
        CanonType(CanonType::DEPRECATED.0 | CanonType::SUPPRESS_SCRIPT.0);
    /// All canonicalizations.
    pub(crate) const ALL: CanonType =
        CanonType(CanonType::BCP47.0 | CanonType::LEGACY.0 | CanonType::MACRO.0);
    /// The canonicalization of `Parse`, `Make` and `Compose`.
    pub(crate) const DEFAULT: CanonType = CanonType(CanonType::DEPRECATED.0 | CanonType::LEGACY.0);

    const CANON_LANG: CanonType =
        CanonType(CanonType::DEPRECATED_BASE.0 | CanonType::LEGACY.0 | CanonType::MACRO.0);

    fn has(self, o: CanonType) -> bool {
        self.0 & o.0 != 0
    }
}

/// The canonicalized equivalent of the tag and whether anything changed.
pub(crate) fn canonicalize(c: CanonType, mut t: FullTag) -> (FullTag, bool) {
    if c == CanonType::RAW {
        return (t, false);
    }
    let mut changed = false;
    if c.has(CanonType::SUPPRESS_SCRIPT) && t.lang_id.suppress_script() == t.script_id {
        t.script_id = ScriptId(0);
        changed = true;
    }
    if c.has(CanonType::CANON_LANG) {
        loop {
            let (l, alias_type) = t.lang_id.canonicalize();
            if l != t.lang_id {
                match alias_type {
                    AliasType::Legacy => {
                        if c.has(CanonType::LEGACY) {
                            if t.lang_id == LangId(LANG_SH) && t.script_id.0 == 0 {
                                t.script_id = ScriptId(SCRIPT_LATN);
                            }
                            t.lang_id = l;
                            changed = true;
                        }
                    }
                    AliasType::Macro => {
                        if c.has(CanonType::MACRO) {
                            // Go deviates from CLDR here and maps "nb" to "no" unless CLDR is set.
                            if !c.has(CanonType::CLDR) || t.lang_id != LangId(LANG_NB) {
                                changed = true;
                                t.lang_id = l;
                            }
                        }
                    }
                    AliasType::Deprecated => {
                        if c.has(CanonType::DEPRECATED_BASE) {
                            if t.lang_id == LangId(LANG_MO) && t.region_id.0 == 0 {
                                t.region_id = RegionId(REGION_MD);
                            }
                            t.lang_id = l;
                            changed = true;
                            // Other canonicalization types may still apply.
                            continue;
                        }
                    }
                    AliasType::Unknown => {}
                }
            } else if c.has(CanonType::LEGACY)
                && t.lang_id == LangId(LANG_NO)
                && c.has(CanonType::CLDR)
            {
                t.lang_id = LangId(LANG_NB);
                changed = true;
            }
            break;
        }
    }
    if c.has(CanonType::DEPRECATED_SCRIPT) && t.script_id == ScriptId(SCRIPT_QAAI) {
        changed = true;
        t.script_id = ScriptId(SCRIPT_ZINH);
    }
    if c.has(CanonType::DEPRECATED_REGION) {
        let r = t.region_id.canonicalize();
        if r != t.region_id {
            changed = true;
            t.region_id = r;
        }
    }
    (t, changed)
}
