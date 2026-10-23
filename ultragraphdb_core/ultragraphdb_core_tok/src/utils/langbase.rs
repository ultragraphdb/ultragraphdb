/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/langbase.go. Changed for ultragraphdb: translated to Rust over
 * ultragraphdb_core_text's port of x/text/language.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

use ultragraphdb_core_text::language_base;

const EN_BASE: &str = "en";

/// Dgraph's `langBaseCache`: language tag to base.
static LANG_BASE_CACHE: LazyLock<RwLock<HashMap<String, String>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Dgraph's `LangBase`: the BCP 47 base of a language tag, or `en` when the tag is empty, does not parse, is
/// undefined or its base is inferred with no confidence. Results are cached, as Dgraph caches them.
///
/// ```
/// use ultragraphdb_core_tok::lang_base;
///
/// assert_eq!(lang_base("zh-Hant"), "zh");
/// assert_eq!(lang_base("sh"), "sr");
/// assert_eq!(lang_base("x-klingon"), "en");
/// ```
pub fn lang_base(lang: &str) -> String {
    if lang.is_empty() {
        return EN_BASE.to_string();
    }
    if let Some(s) = LANG_BASE_CACHE
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(lang)
    {
        return s.clone();
    }
    let base = language_base(lang).unwrap_or_else(|| EN_BASE.to_string());
    LANG_BASE_CACHE
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .entry(lang.to_string())
        .or_insert(base)
        .clone()
}
