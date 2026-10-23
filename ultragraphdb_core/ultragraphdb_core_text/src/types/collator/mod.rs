/*
 * Copyright 2012 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, collate/collate.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod collator_compare;
mod collator_key;

use std::sync::LazyLock;

use crate::tables::collate_tables::{AVAILABLE_LOCALES, LOCALES};
use crate::types::numeric_weighter::NumericWeighter;
use crate::types::options::Options;
use crate::types::table::Table;
use crate::types::tag::Tag;
use crate::types::weighter::Weighter;
use crate::utils::canonicalize::CanonType;
use crate::utils::match_lang::match_lang;

/// The locales with a tailoring of their own, parsed without canonicalization: `und` first, then sorted.
static TAGS: LazyLock<Vec<Tag>> = LazyLock::new(|| {
    AVAILABLE_LOCALES
        .split(',')
        .map(|s| {
            Tag::parse_with(CanonType::RAW, s)
                .unwrap_or_else(|e| panic!("collation locale {s}: {e}"))
        })
        .collect()
});

/// Compares and sorts strings in the collation order of a language, as Go's `collate.Collator` with the CLDR 23
/// tables. Keys are Go's own format: primary weights, then `00 00` and the secondary weights, then `00 00` and
/// the tertiary weights.
#[derive(Debug, Clone)]
pub struct Collator {
    options: Options,
}

impl Collator {
    /// `collate.New(tag)` without options: the collator of the tailoring that best matches the tag, with the
    /// collation options of the tag's `-u` extension (`kc`, `kb`, `kn`, `ks`, `ka`).
    pub fn new(tag: &Tag) -> Collator {
        let index = match_lang(tag, &TAGS);
        let mut options = Options::new(Weighter::Table(Table::new(LOCALES[index])));
        // Set options from the user-supplied tag.
        options.set_from_tag(tag);
        if options.numeric
            && let Weighter::Table(t) = options.t
        {
            options.t = Weighter::Numeric(NumericWeighter::new(t));
        }
        Collator { options }
    }
}
