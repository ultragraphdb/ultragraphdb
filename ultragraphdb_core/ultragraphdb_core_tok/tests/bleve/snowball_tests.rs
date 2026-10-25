/*
 * Copyright (c) 2018 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * The cases of bleve v2.5.7's analysis/lang/ru/stemmer_ru_test.go and analysis/lang/tr/stemmer_tr_test.go are
 * ported. Changed for ultragraphdb: translated to Rust; the nine Snowball stemmers Dgraph names in one table.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! bleve tests the Russian and Turkish stemmers only; every other expected stem is bleve's filters' output, by a Go
//! program over snowballstem v0.9.0, on words of the corpus sentences and on the edge inputs.

use crate::bleve::check_stems;

/// A stemmer's name, its (word, stem) cases, and the edge inputs it changes.
type Case<'a> = (&'a str, &'a [(&'a str, &'a str)], &'a [(&'a str, &'a str)]);

#[test]
fn test_snowball_stemmers() {
    let tests: [Case; 9] = [
        (
            "stemmer_da_snowball",
            &[
                ("børnene", "børn"),
                ("legede", "leged"),
                ("haverne", "hav"),
                ("hjemme", "hjem"),
                ("bøgerne", "bøg"),
            ],
            &[],
        ),
        (
            "stemmer_fi_snowball",
            &[
                ("lapset", "laps"),
                ("leikkivät", "leikkiv"),
                ("iloisesti", "ilois"),
                ("kaupungin", "kaupung"),
                ("taloissamme", "talo"),
            ],
            &[],
        ),
        (
            "stemmer_hu_snowball",
            &[
                ("gyerekek", "gyerek"),
                ("vidáman", "vida"),
                ("játszottak", "játszott"),
                ("kertjeiben", "kert"),
                ("házainkban", "ház"),
            ],
            &[],
        ),
        (
            "stemmer_nl_snowball",
            &[
                ("kinderen", "kinder"),
                ("speelden", "speeld"),
                ("vrolijk", "vrolijk"),
                ("tuinen", "tuin"),
                ("huizen", "huiz"),
            ],
            &[("é", "e"), ("aé", "ae")],
        ),
        (
            "stemmer_no_snowball",
            &[
                ("barna", "barn"),
                ("lekte", "lekt"),
                ("lykkelig", "lykk"),
                ("hagene", "hag"),
                ("husene", "hus"),
            ],
            &[],
        ),
        (
            "stemmer_ro_snowball",
            &[
                ("copiii", "copii"),
                ("jucau", "jucau"),
                ("fericiți", "fericiț"),
                ("grădinile", "grădin"),
                ("orașului", "oraș"),
            ],
            &[],
        ),
        (
            "stemmer_ru_snowball",
            &[
                ("актеров", "актер"),
                ("километров", "километр"),
                ("дети", "дет"),
                ("весело", "весел"),
                ("играли", "игра"),
                ("больших", "больш"),
                ("садах", "сад"),
            ],
            &[],
        ),
        (
            "stemmer_sv_snowball",
            &[
                ("barnen", "barn"),
                ("lekte", "lekt"),
                ("glatt", "glatt"),
                ("trädgårdarna", "trädgård"),
                ("staden", "stad"),
            ],
            &[],
        ),
        (
            "stemmer_tr_snowball",
            &[
                ("kimsesizler", "kimsesiz"),
                ("kitaplar", "kitap"),
                ("arabanın", "araba"),
                ("bardaklar", "bardak"),
                ("kediye", "kedi"),
                ("yazdım", "yaz"),
                ("çocuklar", "çocuk"),
                ("şehrin", "şehr"),
                ("bahçelerinde", "bahçe"),
                ("neşeyle", "neşe"),
                ("evlerimizdeyiz", "ev"),
            ],
            &[("eed", "eedi")],
        ),
    ];
    for (name, cases, changed) in tests {
        check_stems(name, cases, changed);
    }
}
