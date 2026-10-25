/*
 * Copyright (c) 2014 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/lang/hi/hindi_stemmer_filter_test.go. Changed for ultragraphdb: translated to Rust;
 * every case of the Go table, through the filter Dgraph names, beside the edge inputs with Go's stems.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::bleve::check_stems;

#[test]
fn test_hindi_stemmer_filter() {
    check_stems(
        "stemmer_hi",
        &[
            ("लडका", "लडक"),
            ("लडके", "लडक"),
            ("लडकों", "लडक"),
            ("गुरु", "गुर"),
            ("गुरुओं", "गुर"),
            ("दोस्त", "दोस्त"),
            ("दोस्तों", "दोस्त"),
            ("लडकी", "लडक"),
            ("लडकियों", "लडक"),
            ("किताब", "किताब"),
            ("किताबें", "किताब"),
            ("किताबों", "किताब"),
            ("आध्यापीका", "आध्यापीक"),
            ("आध्यापीकाएं", "आध्यापीक"),
            ("आध्यापीकाओं", "आध्यापीक"),
            ("खाना", "खा"),
            ("खाता", "खा"),
            ("खाती", "खा"),
            ("खा", "खा"),
            ("कठिनाइयां", "कठिन"),
            ("कठिन", "कठिन"),
            ("", ""),
        ],
        &[],
    );
}
