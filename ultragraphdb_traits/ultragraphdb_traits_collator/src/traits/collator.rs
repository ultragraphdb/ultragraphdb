/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cmp::Ordering;

/// The language collation Dgraph's sort uses for strings with a language (Go's
/// `collate.New(language.Parse(lang))`, from `golang.org/x/text`); `ultragraphdb_core_types::TextCollator` is
/// `ultragraphdb_core_text`'s port of it.
///
/// A sort builds one [`Collator::Language`] per call, as Dgraph builds one `collate.Collator`, and compares strings
/// with it as Go's `Collator.CompareString` does. Comparing collation keys instead gives a different order under
/// some tags' collation options (`-u-ka-shifted`, `-u-ka-blanked`, `-u-ks-identic`).
pub trait Collator {
    /// The collation order of one language.
    type Language;

    /// The collation order of `lang`; `None` when Go's `language.Parse` refuses `lang`, in which case Dgraph
    /// compares the strings' bytes.
    fn language(&self, lang: &str) -> Option<Self::Language>;

    /// Compares two strings in the collation order of `language`, as Go's `Collator.CompareString`.
    fn compare(&self, language: &Self::Language, a: &str, b: &str) -> Ordering {
        self.compare_bytes(language, a.as_bytes(), b.as_bytes())
    }

    /// Compares two strings' bytes in the collation order of `language`, as Go's `Collator.CompareString` over Go
    /// strings holding them: the bytes need not be valid UTF-8, as Dgraph's string values need not be.
    fn compare_bytes(&self, language: &Self::Language, a: &[u8], b: &[u8]) -> Ordering;
}
