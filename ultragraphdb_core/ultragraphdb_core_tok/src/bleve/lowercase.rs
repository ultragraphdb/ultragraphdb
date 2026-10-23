/*
 * Copyright (c) 2014 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/token/lowercase/lowercase.go. Changed for ultragraphdb: translated to Rust over
 * Go's unicode.ToLower; every rune of the term is kept and the final-sigma rule applies on every path (ledger 16,
 * 30).
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::borrow::Cow;

use ultragraphdb_common_utils::go_unicode;

/// bleve's `toLowerDeferredCopy` as its comment states it, "exactly like bytes.ToLower()" but for its one intended
/// difference: every rune by Go's `unicode.ToLower`, and a `Σ` ending the term as `ς` (bleve's test takes `ὈΔΥΣΣ`
/// to `ὀδυσς`). The term is borrowed when no rune changes.
///
/// bleve itself moves only the runes it changes, so after a rune whose lowercase form is shorter (`İ`, two bytes,
/// to `i`, one) the runes it leaves unchanged keep their old bytes and the term is cut (ledger 16); and after a rune
/// whose lowercase form is wider (`Ⱥ` to `ⱥ`) it hands the rest to `bytes.ToLower`, which has no final-sigma rule
/// (ledger 30). This port does neither.
pub(crate) fn to_lower(term: &str) -> Cow<'_, str> {
    let lower = |(i, c): (usize, char)| {
        let l = go_unicode::to_lower(c);
        // Σ is the one rune that lowercases to σ.
        if l == 0x03C3 && l != u32::from(c) && i + c.len_utf8() == term.len() {
            'ς'
        } else {
            char::from_u32(l).unwrap_or(c)
        }
    };
    if term.char_indices().all(|ic| lower(ic) == ic.1) {
        return Cow::Borrowed(term);
    }
    Cow::Owned(term.char_indices().map(lower).collect())
}
