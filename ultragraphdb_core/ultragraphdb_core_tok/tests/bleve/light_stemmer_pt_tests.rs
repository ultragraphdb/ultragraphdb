/*
 * Copyright (c) 2015 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/lang/pt/light_stemmer_pt_test.go. Changed for ultragraphdb: translated to Rust;
 * every case of the Go table, through the filter Dgraph names, beside the edge inputs with Go's stems.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::bleve::check_stems;

#[test]
fn test_portuguese_light_stemmer() {
    check_stems(
        "stemmer_pt_light",
        &[
            ("doutores", "doutor"),
            ("doutor", "doutor"),
            ("homens", "homem"),
            ("homem", "homem"),
            ("papéis", "papel"),
            ("papel", "papel"),
            ("normais", "normal"),
            ("normal", "normal"),
            ("lencóis", "lencol"),
            ("lencol", "lencol"),
            ("barris", "barril"),
            ("barril", "barril"),
            ("botões", "bota"),
            ("botão", "bota"),
            ("o", "o"),
            ("debate", "debat"),
            ("político", "politic"),
            ("pelo", "pelo"),
            ("menos", "meno"),
            ("o", "o"),
            ("que", "que"),
            ("vem", "vem"),
            ("a", "a"),
            ("público", "public"),
            ("parece", "parec"),
            ("de", "de"),
            ("modo", "modo"),
            ("nada", "nada"),
            ("surpreendente", "surpreendent"),
            ("restrito", "restrit"),
            ("a", "a"),
            ("temas", "tema"),
            ("menores", "menor"),
            ("mas", "mas"),
            ("há", "há"),
            ("evidentemente", "evident"),
            ("grandes", "grand"),
            ("questões", "questa"),
            ("em", "em"),
            ("jogo", "jogo"),
            ("nas", "nas"),
            ("eleições", "eleica"),
            ("que", "que"),
            ("se", "se"),
            ("aproximam", "aproximam"),
        ],
        &[],
    );
}
