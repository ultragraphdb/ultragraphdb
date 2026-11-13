/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use icu_locale_core::Locale;

/// The language subtag of the BCP 47 tag `lang`, in lower case; `None` when `lang` is no well-formed tag or names no
/// language (`und`).
///
/// ```
/// use ultragraphdb_core_text::language_base;
///
/// assert_eq!(language_base("en-US").as_deref(), Some("en"));
/// assert_eq!(language_base("zh-Hant").as_deref(), Some("zh"));
/// assert_eq!(language_base("und"), None);
/// assert_eq!(language_base("x!"), None);
/// ```
#[must_use]
pub fn language_base(lang: &str) -> Option<String> {
    let language = Locale::try_from_str(lang).ok()?.id.language;
    (!language.is_unknown()).then(|| language.as_str().to_string())
}
