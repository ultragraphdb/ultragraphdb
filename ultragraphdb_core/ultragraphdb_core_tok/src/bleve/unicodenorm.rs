/*
 * Copyright (c) 2014 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/token/unicodenorm/unicodenorm.go, with the NFKC form Dgraph defines in
 * tok/bleve.go. Changed for ultragraphdb: translated to Rust over unicode-normalization (Unicode 17.0.0, as Go's
 * x/text v0.41.0 under Go 1.27).
 * Copyright (c) 2026. Marvin Hansen.
 */

use unicode_normalization::UnicodeNormalization;

/// Go's `norm.NFKC.Bytes`. Go's normalizer keeps text in the Stream-Safe Text Format, inserting U+034F COMBINING
/// GRAPHEME JOINER after 30 non-starters in a row, so the text is made stream-safe before it is composed.
///
/// An invalid UTF-8 byte is a normalization boundary that Go copies as it is; the lowercase filter before this one
/// leaves such bytes in some terms (see `to_lower_deferred_copy`), so each valid run is normalized on its own.
pub(crate) fn nfkc(term: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(term.len());
    for chunk in term.utf8_chunks() {
        out.extend(
            chunk
                .valid()
                .chars()
                .stream_safe()
                .nfkc()
                .collect::<String>()
                .into_bytes(),
        );
        out.extend_from_slice(chunk.invalid());
    }
    out
}
