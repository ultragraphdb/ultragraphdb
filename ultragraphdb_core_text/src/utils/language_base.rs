/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::confidence::Confidence;
use crate::types::tag::Tag;

/// The base language Dgraph's `tok.LangBase` keeps for a language tag: the tag parses without error, is not
/// `und`, and its base is inferred with more than no confidence. `None` where Dgraph falls back to English.
///
/// ```
/// use ultragraphdb_core_text::language_base;
///
/// assert_eq!(language_base("en-US").as_deref(), Some("en"));
/// assert_eq!(language_base("sh").as_deref(), Some("sr"));
/// assert_eq!(language_base("und"), None);
/// assert_eq!(language_base("x!"), None);
/// ```
pub fn language_base(lang: &str) -> Option<String> {
    let tag = Tag::parse(lang).ok()?;
    if tag == Tag::default() {
        return None;
    }
    let (base, conf) = tag.base();
    (conf > Confidence::No).then(|| base.to_string())
}
