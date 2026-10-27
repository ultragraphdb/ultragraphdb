/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use blake2::{Blake2b256, Digest};
use cucumber::{World, then, when};
use serde::Deserialize;
use ultragraphdb_core_tok::{
    IDENT_NGRAM, IDENT_TERM, analyze, build_tokens, get_factory_create_spec_from_spec,
    get_ngram_query_tokens, get_ngram_tokens, get_tokenizer, get_tokenizer_for_lang, lang_base,
};
use ultragraphdb_core_types::{TypeId, Val, Value, convert};
use ultragraphdb_proto_pb::pb;

#[derive(Debug, Default, World)]
struct Steps {
    tokens: Vec<Vec<u8>>,
    groups: Vec<Vec<Vec<u8>>>,
    texts: Vec<String>,
    mismatches: Vec<String>,
    compared: usize,
    deviating: usize,
    flag: bool,
    error: String,
}

#[derive(Debug, Clone, Deserialize)]
struct TokensCase {
    tokenizer: String,
    lang: String,
    via: String,
    kind: String,
    input: String,
    tokens: Vec<String>,
    sorted: bool,
    error: String,
    /// The port's tokens where they differ from Dgraph's; the unit test `test_tokens_golden` names each such case
    /// with its ledger entry.
    #[serde(default)]
    fixed: Option<Vec<String>>,
}

fn golden() -> Vec<TokensCase> {
    let path = ultragraphdb_tool_bdd::testdata_dir().join("golden/tok_tokens.json");
    ultragraphdb_tool_bdd::load_golden::<TokensCase>(&path)
        .cases()
        .to_vec()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

/// The golden cases whose port tokens differ from Dgraph's (ledger 14, 16, 30).
const DEVIATING: usize = 366;

fn hex(b: &[u8]) -> String {
    b.iter().map(|b| format!("{b:02x}")).collect()
}

fn untext(s: &str) -> String {
    String::from_utf8(unhex(s)).unwrap()
}

fn enc(id: u8, s: &[u8]) -> Vec<u8> {
    let mut v = vec![id];
    v.extend_from_slice(s);
    v
}

fn value(c: &TokensCase) -> Value {
    let from_text = |to| {
        convert(
            &Val::new(TypeId::STRING, Value::Bytes(c.input.as_bytes().to_vec())),
            to,
        )
        .unwrap()
        .into_value()
        .unwrap()
    };
    match c.kind.as_str() {
        "string" => Value::Str(untext(&c.input)),
        "int" => Value::Int(c.input.parse().unwrap()),
        "float" => Value::Float(f64::from_bits(u64::from_str_radix(&c.input, 16).unwrap())),
        "bool" => Value::Bool(c.input == "true"),
        "vfloat" => Value::VFloat(vec![1.0, 2.0, 3.0]),
        "bigfloat" => from_text(TypeId::BIGFLOAT),
        "datetime" => from_text(TypeId::DATETIME),
        _ => from_text(TypeId::GEO),
    }
}

/// Compares a golden case with the port.
fn compare(c: &TokensCase) -> Result<(), String> {
    let got: Result<Vec<Vec<u8>>, String> = match c.via.as_str() {
        "build" => build_tokens(
            &value(c),
            &get_tokenizer_for_lang(&get_tokenizer(&c.tokenizer).unwrap(), &c.lang),
        )
        .map_err(|e| e.to_string()),
        "ngram" => Ok(get_ngram_tokens(untext(&c.input), &c.lang)),
        _ => Ok(get_ngram_query_tokens(untext(&c.input), &c.lang)),
    };
    let got = got.map(|mut t| {
        if c.sorted {
            t.sort();
        }
        t
    });
    let want = if c.error.is_empty() {
        Ok(c.fixed
            .as_ref()
            .unwrap_or(&c.tokens)
            .iter()
            .map(|t| unhex(t))
            .collect())
    } else {
        Err(c.error.clone())
    };
    if got == want {
        Ok(())
    } else {
        Err(format!(
            "{} {:?} {}: want {want:?}, got {got:?}",
            c.tokenizer, c.lang, c.input
        ))
    }
}

fn compare_all(w: &mut Steps, cases: impl Iterator<Item = TokensCase>) {
    for c in cases {
        w.compared += 1;
        w.deviating += usize::from(c.fixed.is_some());
        if let Err(e) = compare(&c) {
            w.mismatches.push(e);
        }
    }
}

fn tokens_of(name: &str, lang: &str, s: &str) -> Vec<Vec<u8>> {
    build_tokens(
        &Value::str(s),
        &get_tokenizer_for_lang(&get_tokenizer(name).unwrap(), lang),
    )
    .unwrap()
}

fn no_mismatches(w: &mut Steps) {
    assert!(w.compared > 0, "nothing compared");
    assert!(
        w.mismatches.is_empty(),
        "{} of {} differ:\n{}",
        w.mismatches.len(),
        w.compared,
        w.mismatches[..w.mismatches.len().min(5)].join("\n")
    );
}

#[when(expr = "each tokenizer tokenizes its golden inputs")]
fn every_tokenizer(w: &mut Steps) {
    compare_all(w, golden().into_iter());
}

#[then(expr = "every token, identifier byte included, equals Dgraph's")]
fn every_token_equal(w: &mut Steps) {
    no_mismatches(w);
    assert_eq!(w.deviating, DEVIATING);
    assert!(w.compared > 15_000);
}

#[when(expr = "the ints MinInt64, -1, 0, 1 and MaxInt64 are tokenized")]
fn ints_tokenized(w: &mut Steps) {
    let int = get_tokenizer("int").unwrap();
    w.tokens = [i64::MIN, -1, 0, 1, i64::MAX]
        .iter()
        .map(|i| build_tokens(&Value::Int(*i), &int).unwrap().remove(0))
        .collect();
}

#[then(expr = "the tokens' byte order is the values' order")]
fn ints_ordered(w: &mut Steps) {
    assert!(w.tokens.windows(2).all(|p| p[0] < p[1]), "{:?}", w.tokens);
}

#[when(expr = "the floats -2.7 and 2.7 are tokenized")]
fn floats_tokenized(w: &mut Steps) {
    let float = get_tokenizer("float").unwrap();
    w.tokens = [-2.7, 2.7]
        .iter()
        .map(|f| float.tokens(&Value::Float(*f)).unwrap().remove(0))
        .collect();
}

#[then(expr = "their tokens equal the int tokens of -2 and 2")]
fn floats_truncated(w: &mut Steps) {
    let int = get_tokenizer("int").unwrap();
    let want: Vec<Vec<u8>> = [-2, 2]
        .iter()
        .map(|i| int.tokens(&Value::Int(*i)).unwrap().remove(0))
        .collect();
    assert_eq!(w.tokens, want);
}

#[when(expr = "{string} is tokenized by the hour tokenizer")]
fn hour_tokenized(w: &mut Steps, s: String) {
    let t = convert(
        &Val::new(TypeId::STRING, Value::Bytes(s.into_bytes())),
        TypeId::DATETIME,
    )
    .unwrap()
    .into_value()
    .unwrap();
    w.tokens = build_tokens(&t, &get_tokenizer("hour").unwrap()).unwrap();
}

#[then(expr = "the token encodes year 2017, month 1, day 1 and hour 11")]
fn hour_fields(w: &mut Steps) {
    assert_eq!(w.tokens, vec![vec![0x43, 0x07, 0xE1, 0, 1, 0, 1, 0, 11]]);
}

#[when(expr = "{string} is tokenized by the term tokenizer")]
fn term_tokenized(w: &mut Steps, s: String) {
    w.tokens = tokens_of("term", "", &s);
}

#[then(expr = "the tokens are {string} and {string}, each prefixed with 0x01")]
fn term_tokens(w: &mut Steps, a: String, b: String) {
    assert_eq!(
        w.tokens,
        vec![enc(IDENT_TERM, a.as_bytes()), enc(IDENT_TERM, b.as_bytes())]
    );
}

#[when(expr = "{string} is tokenized by the term tokenizer for {string}")]
fn term_lang_tokenized(w: &mut Steps, s: String, lang: String) {
    w.tokens = tokens_of("term", &lang, &s);
}

#[then(expr = "the tokens are {string} and {string}")]
fn tokens_are(w: &mut Steps, a: String, b: String) {
    let mut want = vec![a.into_bytes(), b.into_bytes()];
    want.sort();
    let got: Vec<Vec<u8>> = w.tokens.iter().map(|t| t[1..].to_vec()).collect();
    assert_eq!(got, want);
}

#[when(expr = "{string} is tokenized by the fulltext tokenizer for {string}")]
fn fulltext_tokenized(w: &mut Steps, s: String, lang: String) {
    w.tokens = tokens_of("fulltext", &lang, &s);
}

#[when(expr = "{string} is tokenized by the trigram tokenizer")]
fn trigram_tokenized(w: &mut Steps, s: String) {
    w.texts = vec![s.clone()];
    w.tokens = tokens_of("trigram", "", &s);
}

#[then(expr = "the result is Dgraph's 11 trigrams, sorted")]
fn eleven_trigrams(w: &mut Steps) {
    let mut want: Vec<Vec<u8>> = [
        "Dgr", "gra", "rap", "aph", "ph ", "h r", " ro", "roc", "ock", "cks", "ks!",
    ]
    .iter()
    .map(|s| enc(0x0A, s.as_bytes()))
    .collect();
    want.sort();
    assert_eq!(w.tokens, want);
}

#[then(expr = "the tokens are its three-byte windows, {string} among them")]
fn byte_windows(w: &mut Steps, _shown: String) {
    assert_eq!(w.texts, vec!["Grüße".to_string()]);
    // "Grüße" is G r C3 BC C3 9F e: five windows, two of which split a rune, sorted bytewise.
    let want: Vec<Vec<u8>> = [
        &b"Gr\xc3"[..],
        b"r\xc3\xbc",
        b"\xbc\xc3\x9f",
        b"\xc3\x9fe",
        b"\xc3\xbc\xc3",
    ]
    .iter()
    .map(|b| enc(0x0A, b))
    .collect();
    assert_eq!(w.tokens, want);
}

#[when(expr = "the golden Chinese sentence is tokenized by the fulltext tokenizer for {string}")]
fn chinese_fulltext(w: &mut Steps, lang: String) {
    let hex: String = "他是一个薪水很高的商人"
        .bytes()
        .map(|b| format!("{b:02x}"))
        .collect();
    let cases: Vec<TokensCase> = golden()
        .into_iter()
        .filter(|c| {
            c.tokenizer == "fulltext" && c.lang == lang && c.via == "build" && c.input == hex
        })
        .collect();
    assert_eq!(cases.len(), 1);
    w.tokens = tokens_of("fulltext", &lang, "他是一个薪水很高的商人");
    w.groups = vec![cases[0].tokens.iter().map(|t| unhex(t)).collect()];
}

#[then(expr = "the tokens equal Dgraph's bigrams")]
fn chinese_bigrams(w: &mut Steps) {
    assert_eq!(w.tokens, w.groups[0]);
    assert!(
        w.tokens.iter().all(|t| t.len() == 1 + 6),
        "two CJK runes each"
    );
}

#[when(expr = "the golden sentences for every language with a stemmer are tokenized by fulltext")]
fn stemmed_languages(w: &mut Steps) {
    const STEMMED: [&str; 21] = [
        "ar", "ckb", "da", "de", "en", "es", "fi", "fr", "hi", "hu", "it", "ja", "ko", "nl", "no",
        "pt", "ro", "ru", "sv", "tr", "zh",
    ];
    let cases: Vec<TokensCase> = golden()
        .into_iter()
        .filter(|c| {
            c.tokenizer == "fulltext" && c.via == "build" && STEMMED.contains(&c.lang.as_str())
        })
        .collect();
    let langs: std::collections::BTreeSet<String> = cases.iter().map(|c| c.lang.clone()).collect();
    assert_eq!(langs.len(), STEMMED.len(), "{langs:?}");
    compare_all(w, cases.into_iter());
}

#[then(expr = "each language's tokens equal Dgraph's")]
fn each_language(w: &mut Steps) {
    no_mismatches(w);
}

#[when(expr = "a sentence yielding a shingle of 30 or more bytes is tokenized by ngram")]
fn long_shingle(w: &mut Steps) {
    w.tokens = get_ngram_tokens("supercalifragilisticexpialidocious fox", "en");
    // Porter's stem of the long word, which step 4 strips of "ous": 31 bytes.
    w.texts = vec!["supercalifragilisticexpialidoci".to_string()];
}

#[then(expr = "that shingle appears as its BLAKE2b-256 digest and shorter shingles appear as text")]
fn shingle_hashed(w: &mut Steps) {
    let stem = &w.texts[0];
    let shingle = format!("{stem} fox");
    assert!(stem.len() >= 30 && shingle.len() >= 30);
    // Dgraph's three tokens: the stem and the bigram as digests, "fox" as text.
    let mut want = vec![
        enc(IDENT_NGRAM, &Blake2b256::digest(stem.as_bytes())),
        enc(IDENT_NGRAM, &Blake2b256::digest(shingle.as_bytes())),
        enc(IDENT_NGRAM, b"fox"),
    ];
    want.sort();
    assert_eq!(w.tokens, want);
    assert_eq!(
        hex(&w.tokens[0]),
        "0f00d2dbb71439248c09fa1509c9bb01f3b5f627dbba86300532e80b61b8535226"
    );
}

#[when(expr = "the two-word query {string} is tokenized for ngram queries")]
fn short_query(w: &mut Steps, q: String) {
    w.texts = vec![q.clone()];
    w.tokens = get_ngram_query_tokens(&q, "en");
}

#[then(expr = "the query tokens are the one shingle of both words")]
fn one_shingle(w: &mut Steps) {
    assert_eq!(w.tokens, vec![enc(IDENT_NGRAM, w.texts[0].as_bytes())]);
}

#[when(expr = "{string}, {string}, {string}, {string} and {string} are reduced")]
fn reduced(w: &mut Steps, a: String, b: String, c: String, d: String, e: String) {
    w.texts = [a, b, c, d, e].iter().map(|t| lang_base(t)).collect();
}

#[then(expr = "the results are {string}, {string}, {string}, {string} and {string}")]
fn reduced_to(w: &mut Steps, a: String, b: String, c: String, d: String, e: String) {
    assert_eq!(w.texts, vec![a, b, c, d, e]);
}

#[when(
    expr = "the golden German strings are tokenized by the exact tokenizer with language {string}"
)]
fn german_exact(w: &mut Steps, lang: String) {
    const GERMAN: [&str; 19] = [
        "Äpfel", "Apfel", "apfel", "Zebra", "zebra", "Öl", "Ofen", "Straße", "Strasse", "Müller",
        "Mueller", "Muller", "ß", "", "a b", "Ärger", "Arzt", "über", "Ufer",
    ];
    let hexes: Vec<String> = GERMAN
        .iter()
        .map(|s| s.bytes().map(|b| format!("{b:02x}")).collect())
        .collect();
    let cases: Vec<TokensCase> = golden()
        .into_iter()
        .filter(|c| c.tokenizer == "exact" && c.lang == lang && hexes.contains(&c.input))
        .collect();
    assert!(cases.len() >= GERMAN.len());
    compare_all(w, cases.into_iter());
}

#[then(expr = "the tokens equal Dgraph's, byte for byte")]
fn exact_equal(w: &mut Steps) {
    no_mismatches(w);
}

#[when(expr = "the golden points and polygons are tokenized by the geo tokenizer")]
fn geo_golden(w: &mut Steps) {
    let cases: Vec<TokensCase> = golden()
        .into_iter()
        .filter(|c| c.tokenizer == "geo")
        .collect();
    assert!(
        cases
            .iter()
            .any(|c| c.input.contains("Polygon") && c.error.is_empty())
    );
    compare_all(w, cases.into_iter());
}

#[then(expr = "the tokens, sorted, equal Dgraph's")]
fn geo_equal(w: &mut Steps) {
    no_mismatches(w);
}

#[when(expr = "the geo tokenizer is checked against the bool type")]
fn geo_vs_bool(w: &mut Steps) {
    w.flag = get_tokenizer("geo").unwrap().is_valid_for(TypeId::BOOL);
}

#[then(expr = "the registry reports it as not valid")]
fn not_valid(w: &mut Steps) {
    assert!(!w.flag);
}

#[when(expr = "the hnsw factory is given metric {string}")]
fn hnsw_metric(w: &mut Steps, metric: String) {
    let spec = pb::VectorIndexSpec {
        name: "hnsw".into(),
        options: vec![pb::OptionPair {
            key: "metric".into(),
            value: metric,
        }],
    };
    w.error = get_factory_create_spec_from_spec(&spec)
        .unwrap_err()
        .to_string();
}

#[then(expr = "the options are refused with {string}")]
fn refused_with(w: &mut Steps, msg: String) {
    assert_eq!(w.error, msg);
}

#[when(expr = "{string} tagged {string} is tokenized for the ngram index and for an ngram query")]
fn ngram_index_and_query(w: &mut Steps, s: String, lang: String) {
    let ngram = get_tokenizer_for_lang(&get_tokenizer("ngram").unwrap(), &lang);
    w.groups = vec![
        build_tokens(&Value::str(s.as_str()), &ngram).unwrap(),
        get_ngram_query_tokens(&s, &lang),
    ];
}

#[then(expr = "both yield the same stemmed shingles, starting with {string}")]
fn same_shingles(w: &mut Steps, stem: String) {
    let (index, query) = (&w.groups[0], &w.groups[1]);
    // Go's GetNGramTokens and GetNGramQueryTokens for "de": "und" is a stop word, the rest stemmed.
    let shingle = format!("{stem} auffassung");
    assert_eq!(
        index,
        &vec![
            enc(IDENT_NGRAM, b"auffassung"),
            enc(IDENT_NGRAM, stem.as_bytes()),
            enc(IDENT_NGRAM, shingle.as_bytes())
        ]
    );
    assert_eq!(query, &vec![enc(IDENT_NGRAM, shingle.as_bytes())]);
    assert!(
        query.iter().all(|q| index.contains(q)),
        "query tokens {query:?} are not all index tokens {index:?}"
    );
}

#[then(expr = "the token is the term {string}")]
fn the_term(w: &mut Steps, term: String) {
    assert_eq!(w.tokens, vec![enc(IDENT_TERM, term.as_bytes())]);
}

#[when(expr = "{string} is lowercased")]
fn lowercased(w: &mut Steps, s: String) {
    w.texts = analyze(&s).iter().map(|t| t.term().to_string()).collect();
}

#[then(expr = "the result is {string}")]
fn the_result(w: &mut Steps, want: String) {
    assert_eq!(w.texts, vec![want]);
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()),
    );
}
