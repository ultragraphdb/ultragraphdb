/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! bleve v2.5.7's `stemmer_es_light` has no test of its own; the expected stems are its output on these words, from
//! Dgraph's tok/stemmers_test.go and the corpus sentences, by a Go program over bleve's filter.

use crate::bleve::check_stems;

#[test]
fn test_spanish_light_stemmer() {
    check_stems(
        "stemmer_es_light",
        &[
            ("deseándoles", "deseándol"),
            ("muchas", "much"),
            ("alegrías", "alegrí"),
            ("señoritas", "señorit"),
            ("señores", "señor"),
            ("programadores", "programador"),
            ("niños", "niñ"),
            ("corriendo", "corriend"),
            ("rápidamente", "rápidament"),
            ("calles", "call"),
            ("ciudades", "ciudad"),
            ("luces", "luz"),
            ("canciones", "cancion"),
            ("veces", "vez"),
        ],
        &[],
    );
}
