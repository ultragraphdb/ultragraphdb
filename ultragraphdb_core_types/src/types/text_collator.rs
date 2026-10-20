/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/sort.go (the collator SortTopN and SortWithFacet build). Changed for
 * ultragraphdb: translated to Rust, over ultragraphdb_core_text's port of golang.org/x/text.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::cmp::Ordering;

use ultragraphdb_core_text::Tag;

use crate::traits::collator::Collator;

/// Dgraph's language collation: `collate.New(tag)` for a tag `language.Parse` accepts, compared with
/// `CompareString`, from `ultragraphdb_core_text`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextCollator;

impl Collator for TextCollator {
    type Language = ultragraphdb_core_text::Collator;

    fn language(&self, lang: &str) -> Option<Self::Language> {
        Tag::parse(lang)
            .ok()
            .map(|tag| ultragraphdb_core_text::Collator::new(&tag))
    }

    fn compare_bytes(&self, language: &Self::Language, a: &[u8], b: &[u8]) -> Ordering {
        language.compare_bytes(a, b)
    }
}
