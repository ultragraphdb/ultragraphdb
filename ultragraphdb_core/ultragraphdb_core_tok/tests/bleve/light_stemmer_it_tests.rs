/*
 * Copyright (c) 2015 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/lang/it/light_stemmer_it_test.go. Changed for ultragraphdb: translated to Rust;
 * every case of the Go table, through the filter Dgraph names, beside the edge inputs with Go's stems.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::bleve::check_stems;

#[test]
fn test_italian_light_stemmer() {
    check_stems(
        "stemmer_it_light",
        &[("ragazzo", "ragazz"), ("ragazzi", "ragazz")],
        &[],
    );
}
