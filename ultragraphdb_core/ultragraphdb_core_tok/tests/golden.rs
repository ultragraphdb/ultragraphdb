/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Loading the golden vectors of tools/golden (tok.go, tok_analysis.go), and the inputs on which the port's
//! analysis differs from Dgraph's.

use serde::de::DeserializeOwned;

/// The cases of `testdata/golden/<name>.json`.
pub(crate) fn cases<T: DeserializeOwned + Clone>(name: &str) -> Vec<T> {
    let path = ultragraphdb_test_bdd::testdata_dir().join(format!("golden/{name}.json"));
    ultragraphdb_test_bdd::load_golden::<T>(&path)
        .cases()
        .to_vec()
}

/// The bytes of a hex string.
pub(crate) fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

/// The text of a hex string; the generator writes valid UTF-8 only, as every string value is.
pub(crate) fn untext(s: &str) -> String {
    String::from_utf8(unhex(s)).expect("the golden text is UTF-8")
}

/// The golden inputs whose analyzed terms differ from Dgraph's, with the ledger entry
/// (`openspec/notes/dgraph-bugs-found-in-port.md`): bleve's lowercase filter leaves stale bytes after a rune whose
/// lowercase form is shorter (16), and skips the final-sigma rule after a rune whose lowercase form is wider (30).
/// The port's result is the case's `fixed` field, which the generator writes with the lowercasing bleve's comment
/// and test state (`tokToLower` in `tools/golden/tok_analysis.go`); `tok_analysis`, `tok_cjk_bigram` and the
/// term, fulltext and ngram cases of `tok_tokens` hold it for exactly these inputs.
pub(crate) const LOWERCASE_DEVIATIONS: &[(&str, u32)] = &[
    ("ȺȾ Ⱥbc aȺ ȺΣ", 30),
    ("ב\rς＿בะЖتあ\u{ad}한ث🍕ⅫΩ\u{200c}é三イİб", 16),
    ("\u{85}大\"Ⅻ\tה二вİ·y\u{308}中Ⅻｲ", 16),
    ("\u{200c}а二õXกвคあİÿ＿׳Ⱦ\u{b}ค가9あα", 16),
    ("イİב", 16),
    ("א5ก\u{94d}İàé", 16),
    ("\u{300}ב①׳あİς한", 16),
    (
        "\u{308}İあ4一\u{2028}❤б한ÿİ\u{323}\t\u{200c}คג\u{b}\u{ad}",
        16,
    ),
    ("\u{300}·׳׳คİج⁀İ׳글1Я❤c", 16),
    ("·イि2👍ﬂ🏽ﬂİ׳ה\u{ad}कう.\u{300}9جค다", 16),
    (" б가דİ\u{94d}ȺȺ;4ข 8", 16),
    ("ー׳6ア/ 🇨ะ⁀ﬁا\u{2028}\u{200b}βİÆ글\u{323}一가a", 16),
    ("ค\u{94d}İ׳글＿ﬂ‿🏽🍕글\u{200d}ाàİ0ा🇦Ａвج׳", 16),
    ("0다人👍三あ\u{200b}/🇧\u{308}İ나🏽ะ\u{c}\u{2028}βข׳", 16),
    ("à\u{200c}İﬂς🇨Ⱥ⁀ \u{b}าب\u{2060}👍Ⱦت\tगา·ゝب", 16),
    ("あ\u{301}аب;اिדİا🇦b", 16),
    ("דجcい‿״?🇨ゝ\u{308}\u{85}ｲ Zｱ Я1İy\u{85}", 16),
    ("·جｱ5\u{308}İа글\u{301}", 16),
    ("ゝȺCΣ❤Ⱥウ\u{323}ゝ한вȺ׳אج", 30),
    ("Ωהا0िकⅫİ״α36\u{2028}", 16),
    ("İ나\u{94d}״글ד\u{200b}ア‿Ａ나Ⱥ γ\u{308}🍕あ👍🇧γ", 16),
    ("二ข二\u{200b}α👍글بİ4İ\u{300}ה🇨ت׳🇦גBゝ", 16),
    ("İ6าøЯ\rBウßİ2い בتウȾ4\n:ﬂ", 16),
    ("Æ!나а\u{c}าЖ三❤İ:क\u{200d}う\tا·0\u{941}イõ", 16),
    ("\u{301}🍕🇨#àв👍ＡYб·\rاİ\u{301}0", 16),
    ("１α人三ะبÿ#🇨うбßאİczج＿״う🇦8\tﬂ", 16),
];
