/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cmp::Ordering;
use std::sync::Arc;

use icu_collator::options::CollatorOptions;
use icu_collator::{Collator as IcuCollator, CollatorBorrowed};
use icu_locale_core::Locale;

/// The collation order of one language, from CLDR through ICU4X at its default, tertiary, strength: what a sort by a
/// language compares with, and what an exact index with a language keys its strings by. A clone shares the one
/// collator, which holds only borrowed, compiled data.
#[derive(Debug, Clone)]
pub struct Collator {
    // Shared and immutable: built once per language and read by every comparison.
    collator: Arc<CollatorBorrowed<'static>>,
}

impl Collator {
    /// The collation of the BCP 47 tag `lang`, a tag's `-u-` collation keywords included; a language CLDR holds no
    /// tailoring for collates by the root order. `None` when `lang` is no well-formed tag.
    #[must_use]
    pub fn for_language(lang: &str) -> Option<Self> {
        let locale = Locale::try_from_str(lang).ok()?;
        IcuCollator::try_new((&locale).into(), CollatorOptions::default())
            .ok()
            .map(|collator| Self {
                collator: Arc::new(collator),
            })
    }

    /// Orders `a` and `b` in this collation.
    #[must_use]
    pub fn compare(&self, a: &str, b: &str) -> Ordering {
        self.collator.compare(a, b)
    }

    /// Orders `a` and `b` in this collation, each invalid UTF-8 sequence collating as U+FFFD.
    #[must_use]
    pub fn compare_bytes(&self, a: &[u8], b: &[u8]) -> Ordering {
        self.collator.compare_utf8(a, b)
    }

    /// The sort key of `s`: two keys of one collation compare, as bytes, as their strings collate. Each invalid UTF-8
    /// sequence is keyed as U+FFFD. A key holds for the ICU4X version that wrote it: an index of keys is rebuilt when
    /// that version's collation data changes.
    #[must_use]
    pub fn key_bytes(&self, s: &[u8]) -> Vec<u8> {
        let mut key = Vec::with_capacity(s.len().saturating_mul(2));
        let Ok(()) = self.collator.write_sort_key_utf8_to(s, &mut key);
        key
    }
}
