/*
 * Copyright 2013 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, internal/language/compact/language.go, internal/language/compact/compact.go
 * and internal/language/compact.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::sync::LazyLock;

use super::Tag;
use crate::tables::compact_tables::{CORE_TAGS, SPECIAL_TAGS_STR};
use crate::tables::language_tables::LANG_NO_INDEX_OFFSET;
use crate::types::builder::Builder;
use crate::types::full_tag::FullTag;
use crate::types::lang_id::LangId;
use crate::types::region_id::RegionId;
use crate::types::script_id::ScriptId;
use crate::utils::lookup::get_region_id;
use crate::utils::parse;

const _: () = assert!(CORE_TAGS.len() <= 0xFFFF, "a compact identifier is a u16");
/// The number of core tags, the compact identifier of the first special tag: its low 16 bits, the whole of it by the
/// assertion above, so every identifier is a u16 by construction.
const CORE_TAG_COUNT: u16 = {
    let [low, high, ..] = CORE_TAGS.len().to_le_bytes();
    u16::from_le_bytes([low, high])
};

/// The tags with a compact index after the core tags.
///
/// The constant parses, so no tag falls back to the default; a debug build checks it.
static SPECIAL_TAGS: LazyLock<Vec<FullTag>> = LazyLock::new(|| {
    SPECIAL_TAGS_STR
        .split(' ')
        .map(|t| {
            let parsed = parse::parse(t);
            debug_assert!(
                parsed.is_ok(),
                "the constant SPECIAL_TAGS_STR holds valid tags"
            );
            parsed.unwrap_or_default()
        })
        .collect()
});

/// `GetCompactCore`: the language, script and region packed into one integer, unique per combination.
fn get_compact_core(t: &FullTag) -> Option<u32> {
    if t.lang_id.0 > LANG_NO_INDEX_OFFSET {
        return None;
    }
    Some(
        u32::from(t.lang_id.0) << (8 + 12)
            | u32::from(t.script_id.0) << 12
            | u32::from(t.region_id.0),
    )
}

/// The tag of a packed core.
fn core_tag(c: u32) -> FullTag {
    FullTag::from_ids(
        LangId((c >> 20) as u16),
        ScriptId(((c >> 12) & 0xff) as u16),
        RegionId((c & 0x3ff) as u16),
    )
}

fn get_core_index(t: &FullTag) -> Option<u16> {
    let cci = get_compact_core(t)?;
    // Go's sort.Search over the core tags, by their u16 identifiers.
    let (mut lo, mut hi) = (0u16, CORE_TAG_COUNT);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if CORE_TAGS.get(usize::from(mid)).is_some_and(|&c| c < cci) {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    (CORE_TAGS.get(usize::from(lo)) == Some(&cci)).then_some(lo)
}

/// The internal tag of a compact identifier.
fn id_tag(id: u16) -> FullTag {
    match id.checked_sub(CORE_TAG_COUNT) {
        Some(special) => SPECIAL_TAGS
            .get(usize::from(special))
            .cloned()
            .unwrap_or_default(),
        None => CORE_TAGS
            .get(usize::from(id))
            .map_or_else(FullTag::default, |&c| core_tag(c)),
    }
}

/// The closest compact identifier of an internal tag, and whether it matches exactly.
fn from_tag(t: &FullTag) -> (u16, bool) {
    let mut exact = true;
    let mut t = t.clone();
    let (b, s, r) = t.raw();
    if t.has_string() {
        if t.is_private_use() {
            // We have no entries for user-defined tags.
            return (0, false);
        }
        let mut has_extra = false;
        if t.has_variants() {
            if t.has_extensions() {
                let mut build = Builder::default();
                build.set_tag(&FullTag::from_ids(b, s, r));
                build.add_variant(t.variants());
                exact = false;
                t = build.make();
            }
            has_extra = true;
        } else if t.extension(b'u').is_some() {
            // Strip all but the 'va' entry.
            let old = t.clone();
            let variant = t.type_for_key("va");
            t = FullTag::from_ids(b, s, r);
            if !variant.is_empty() {
                t = match t.set_type_for_key("va", &variant) {
                    Ok(t) | Err((t, _)) => t,
                };
                has_extra = true;
            }
            exact = old == t;
        } else {
            exact = false;
        }
        if has_extra {
            // We have some variants.
            if let Some(id) = (CORE_TAG_COUNT..=u16::MAX)
                .zip(SPECIAL_TAGS.iter())
                .find_map(|(id, special)| (*special == t).then_some(id))
            {
                return (id, exact);
            }
            exact = false;
        }
    }
    if let Some(x) = get_core_index(&t) {
        return (x, exact);
    }
    exact = false;
    if r.0 != 0 && s.0 == 0 {
        // Deal with cases where an extra script is inserted for the region. Go ignores Maximize's error.
        let (t, _) = t.maximize();
        if let Some(x) = get_core_index(&t) {
            return (x, exact);
        }
    }
    t = t.parent();
    while t != FullTag::default() {
        // No variants specified: just compare core components.
        if let Some(x) = get_core_index(&t) {
            return (x, exact);
        }
        t = t.parent();
    }
    (0, exact)
}

impl Tag {
    /// `compact.Make`: the compact form of an internal tag.
    pub(crate) fn make(t: FullTag) -> Tag {
        let region = t.type_for_key("rg");
        if region.len() == 6 && region.get(2..) == Some("zzzz") {
            let mut code = region.as_bytes().get(..2).unwrap_or_default().to_vec();
            if let Ok(r) = get_region_id_iso2(&mut code) {
                let t_full = t.clone();
                let mut t = match t.set_type_for_key("rg", "") {
                    Ok(t) | Err((t, _)) => t,
                };
                // TODO in Go: should we not consider "va" for the language tag?
                let (language, exact1) = from_tag(&t);
                t.region_id = r;
                let (locale, exact2) = from_tag(&t);
                let full = if !exact1 || !exact2 {
                    Some(t_full)
                } else {
                    None
                };
                return Tag {
                    language,
                    locale,
                    full,
                };
            }
        }
        let (lang, ok) = from_tag(&t);
        Tag {
            language: lang,
            locale: lang,
            full: if ok { None } else { Some(t) },
        }
    }

    /// `compact.Tag.Tag`: the internal form of the tag.
    pub(crate) fn to_full(&self) -> FullTag {
        if let Some(full) = &self.full {
            return full.clone();
        }
        let tag = id_tag(self.language);
        if self.language != self.locale {
            let loc = id_tag(self.locale);
            let value = format!("{}zzzz", loc.region_id.as_string().to_lowercase());
            return match tag.set_type_for_key("rg", &value) {
                Ok(t) | Err((t, _)) => t,
            };
        }
        tag
    }

    /// Whether the tag is fully defined by its compact identifiers.
    pub(crate) fn is_compact(&self) -> bool {
        self.full.is_none()
    }

    /// Whether the tag may have extensions; false guarantees it has none.
    pub(crate) fn may_have_extensions(&self) -> bool {
        self.full.is_some()
            || usize::from(self.language) >= CORE_TAGS.len()
            || self.language != self.locale
    }

    /// `compact.Tag.Parent`: the CLDR parent.
    pub(crate) fn compact_parent(&self) -> Tag {
        if let Some(full) = &self.full {
            return Tag::make(full.parent());
        }
        if self.language != self.locale {
            // Simulate stripping -u-rg-xxxxxx
            return Tag {
                language: self.language,
                locale: self.language,
                full: None,
            };
        }
        let (lang, _) = from_tag(&id_tag(self.language).parent());
        Tag {
            language: lang,
            locale: lang,
            full: None,
        }
    }
}

/// `language.ParseRegion` for the two letters of an `rg` value: any parse error is a failure.
fn get_region_id_iso2(code: &mut [u8]) -> Result<RegionId, ()> {
    get_region_id(code).map_err(|_| ())
}
