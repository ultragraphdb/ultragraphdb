/*
 * Copyright 2013 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, internal/language/language.go (type Tag). Changed for ultragraphdb:
 * translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod full_tag_extensions;
mod full_tag_likely;

use crate::types::lang_id::LangId;
use crate::types::region_id::RegionId;
use crate::types::script_id::ScriptId;

/// Equals max lang (3) + script (4) + max reg (3) + 2 dashes.
pub(crate) const MAX_CORE_SIZE: usize = 12;

/// A BCP 47 language tag as `internal/language` holds it: the language, script and region identifiers, and the
/// string form when the tag has variants or extensions. The zero value is `und`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct FullTag {
    pub lang_id: LangId,
    pub region_id: RegionId,
    pub script_id: ScriptId,
    /// Offset of the variants in `str`, including the preceding '-'.
    pub p_variant: u8,
    /// Offset of the first extension in `str`, including the preceding '-'.
    pub p_ext: u16,
    /// The string representation; used only when the tag has variants or extensions.
    pub str: String,
}

impl FullTag {
    /// A tag of the language, script and region identifiers alone.
    pub(crate) fn from_ids(lang_id: LangId, script_id: ScriptId, region_id: RegionId) -> FullTag {
        FullTag {
            lang_id,
            region_id,
            script_id,
            ..FullTag::default()
        }
    }

    /// The language, script and region, without inferring missing ones.
    pub(crate) fn raw(&self) -> (LangId, ScriptId, RegionId) {
        (self.lang_id, self.script_id, self.region_id)
    }

    /// Whether the language, script and region subtags are equal.
    pub(crate) fn equal_tags(&self, a: &FullTag) -> bool {
        self.lang_id == a.lang_id && self.script_id == a.script_id && self.region_id == a.region_id
    }

    /// Whether the tag consists solely of a private-use tag.
    pub(crate) fn is_private_use(&self) -> bool {
        !self.str.is_empty() && self.p_variant == 0
    }

    /// Updates `str` after the language, script or region changed; `p_variant` and `p_ext` still point to the
    /// start of their parts.
    pub(crate) fn remake_string(&mut self) {
        if self.str.is_empty() {
            return;
        }
        let mut extra = self.str[usize::from(self.p_variant)..].to_string();
        if self.p_variant > 0 {
            extra = extra[1..].to_string();
        }
        if self.equal_tags(&FullTag::default()) && extra.starts_with("x-") {
            self.str = extra;
            self.p_variant = 0;
            self.p_ext = 0;
            return;
        }
        let mut b = self.gen_core_bytes();
        if !extra.is_empty() {
            let diff = b.len() as isize - self.p_variant as isize;
            b.push(b'-');
            b.extend_from_slice(extra.as_bytes());
            self.p_variant = (self.p_variant as isize + diff) as u8;
            self.p_ext = (self.p_ext as isize + diff) as u16;
        } else {
            self.p_variant = b.len() as u8;
            self.p_ext = b.len() as u16;
        }
        self.str = String::from_utf8_lossy(&b).into_owned();
    }

    /// The language, script and region subtags, joined by '-'.
    pub(crate) fn gen_core_bytes(&self) -> Vec<u8> {
        let mut b = self.lang_id.as_string().into_bytes();
        if self.script_id.0 != 0 {
            b.push(b'-');
            b.extend_from_slice(self.script_id.as_string().as_bytes());
        }
        if self.region_id.0 != 0 {
            b.push(b'-');
            b.extend_from_slice(self.region_id.as_string().as_bytes());
        }
        b
    }

    /// The canonical string representation of the tag.
    pub(crate) fn as_string(&self) -> String {
        if !self.str.is_empty() {
            return self.str.clone();
        }
        if self.script_id.0 == 0 && self.region_id.0 == 0 {
            return self.lang_id.as_string();
        }
        String::from_utf8_lossy(&self.gen_core_bytes()).into_owned()
    }

    /// The variants, each with its preceding '-', or the empty string.
    pub(crate) fn variants(&self) -> &str {
        if self.p_variant == 0 {
            return "";
        }
        &self.str[usize::from(self.p_variant)..usize::from(self.p_ext)]
    }

    /// Whether the tag defines more than the language, script and region.
    pub(crate) fn has_string(&self) -> bool {
        !self.str.is_empty()
    }

    /// Whether the tag has variants.
    pub(crate) fn has_variants(&self) -> bool {
        u16::from(self.p_variant) < self.p_ext
    }

    /// Whether the tag has extensions.
    pub(crate) fn has_extensions(&self) -> bool {
        usize::from(self.p_ext) < self.str.len()
    }
}
