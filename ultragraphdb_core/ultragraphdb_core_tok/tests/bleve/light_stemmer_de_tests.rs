/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! bleve v2.5.7's `stemmer_de_light` has no test of its own; the expected stems are its output on these words, from
//! the corpus sentences and their plurals, by a Go program over bleve's filter. Umlauts fold, `ß` stays.

use crate::bleve::check_stems;

#[test]
fn test_german_light_stemmer() {
    check_stems(
        "stemmer_de_light",
        &[
            ("katzen", "katz"),
            ("häuser", "haus"),
            ("bäume", "baum"),
            ("gärten", "gart"),
            ("straßen", "straß"),
            ("auffassung", "auffassung"),
            ("auffassungen", "auffassung"),
            ("spielten", "spielt"),
            ("fröhlich", "frohlich"),
            ("großen", "groß"),
            ("ÄÖÜ", "ÄÖÜ"),
            ("kinder", "kind"),
            ("kindern", "kind"),
            ("kindes", "kind"),
        ],
        &[],
    );
}
