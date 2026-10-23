/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use cucumber::{World, given, then, when};
use serde::Deserialize;
use ultragraphdb_core_types::{
    BIG_FLOAT_PRECISION, BigFloat, DateTime, TextCollator, TypeId, Val, Value, convert, encrypt,
    marshal, parse_time, sort, type_for_value, verify_password,
};

#[derive(Debug, Default, World)]
struct Steps {
    ids: Vec<Option<TypeId>>,
    bytes: Vec<u8>,
    failures: Vec<String>,
    error: String,
    bools: Vec<Result<bool, String>>,
    text: String,
    texts: Vec<Result<String, String>>,
    times: Vec<DateTime>,
    types: Vec<TypeId>,
    hash_ok: bool,
    other_fails: bool,
    sorted: Vec<String>,
    golden_lang: Vec<LangSortCase>,
}

fn golden<T: for<'de> Deserialize<'de> + Clone>(name: &str) -> Vec<T> {
    let path = ultragraphdb_test_bdd::testdata_dir().join(format!("golden/{name}.json"));
    ultragraphdb_test_bdd::load_golden::<T>(&path)
        .cases()
        .to_vec()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|c| format!("{c:02x}")).collect()
}

fn binary_of(v: &Val) -> Result<Vec<u8>, String> {
    match marshal(v, TypeId::BINARY)
        .map_err(|e| e.to_string())?
        .into_value()
    {
        Some(Value::Bytes(b)) => Ok(b),
        other => Err(format!("{other:?}")),
    }
}

fn text_of(v: &Val) -> String {
    match marshal(v, TypeId::STRING).unwrap().into_value() {
        Some(Value::Str(s)) => s,
        other => panic!("{other:?}"),
    }
}

#[derive(Debug, Clone, Deserialize)]
struct Rendered {
    #[serde(default)]
    bin: String,
    #[serde(default)]
    panic: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ConvCase {
    from: i32,
    to: i32,
    #[serde(rename = "in")]
    input: String,
    out: Option<Rendered>,
}

/// The golden conversions of a type's binary form to binary: the value's own encoding, as Dgraph writes it.
fn encodings() -> Vec<ConvCase> {
    golden::<ConvCase>("types_convert")
        .into_iter()
        .filter(|c| {
            c.from != TypeId::BINARY.enum_value()
                && c.to == TypeId::BINARY.enum_value()
                && !c.input.starts_with("string:")
                && c.out
                    .as_ref()
                    .is_some_and(|o| !o.bin.is_empty() && o.panic.is_empty())
        })
        .collect()
}

// ---------- Type identifiers ----------

#[when(
    expr = "the names \"int\", \"datetime\", \"uid\", \"bigfloat\" and \"float32vector\" are looked up"
)]
fn look_up_names(w: &mut Steps) {
    w.ids = ["int", "datetime", "uid", "bigfloat", "float32vector"]
        .iter()
        .map(|n| TypeId::for_name(n))
        .collect();
}

#[then(expr = "they map to identifiers 2, 5, 7, 11 and 12")]
fn names_map(w: &mut Steps) {
    let ids: Vec<Option<i32>> = w.ids.iter().map(|t| t.map(TypeId::enum_value)).collect();
    assert_eq!(ids, [Some(2), Some(5), Some(7), Some(11), Some(12)]);
}

// ---------- Binary encodings ----------

#[when(expr = "the golden values of each type are converted to binary")]
fn encode_golden(w: &mut Steps) {
    let cases = encodings();
    assert!(cases.len() > 100, "{} golden encodings", cases.len());
    for c in cases {
        let got = convert(
            &Val::bytes(TypeId::new(c.from), unhex(&c.input)),
            TypeId::BINARY,
        )
        .map(|v| binary_of(&v))
        .map_err(|e| e.to_string());
        let want = c.out.unwrap().bin;
        if got != Ok(Ok(unhex(&want))) {
            w.failures.push(format!(
                "type {} of {}: {got:?}, Dgraph {want}",
                c.from, c.input
            ));
        }
    }
}

#[then(expr = "each result equals Dgraph's bytes")]
fn encodings_match(w: &mut Steps) {
    assert!(w.failures.is_empty(), "{}", w.failures.join("\n"));
}

#[when(expr = "{string} is converted to binary")]
fn datetime_to_binary(w: &mut Steps, s: String) {
    let t = convert(
        &Val::bytes(TypeId::STRING, s.into_bytes()),
        TypeId::DATETIME,
    )
    .unwrap();
    w.bytes = binary_of(&t).unwrap();
}

#[then(expr = "the result is 15 bytes starting with 0x01 and ending in the offset FF FF")]
fn version_1_layout(w: &mut Steps) {
    assert_eq!(w.bytes.len(), 15, "{}", hex(&w.bytes));
    assert_eq!(w.bytes[0], 0x01);
    assert_eq!(&w.bytes[13..], &[0xff, 0xff]);
}

#[when(expr = "each golden binary value is converted back to its type")]
fn decode_golden(w: &mut Steps) {
    for c in encodings() {
        let tid = TypeId::new(c.from);
        let bin = unhex(&c.out.unwrap().bin);
        // A malformed input (a vector of 3 bytes, a datetime with trailing bytes, a string that is not UTF-8) is not a
        // value of its type.
        let Ok(original) = convert(&Val::bytes(tid, unhex(&c.input)), tid) else {
            continue;
        };
        if binary_of(&original) != Ok(bin.clone()) {
            continue;
        }
        let decoded = convert(&Val::bytes(TypeId::BINARY, bin.clone()), tid);
        let ok = match &decoded {
            // NaN floats are equal by their bits only.
            Ok(v) => {
                binary_of(v) == Ok(bin.clone())
                    && (v == &original || matches!(v.value(), Some(Value::Float(f)) if f.is_nan()))
            }
            Err(_) => false,
        };
        if !ok {
            w.failures.push(format!(
                "type {} of {}: {decoded:?}, original {original:?}",
                c.from, c.input
            ));
        }
    }
}

#[then(expr = "the result equals the original value")]
fn decoded_match(w: &mut Steps) {
    assert!(w.failures.is_empty(), "{}", w.failures.join("\n"));
}

// ---------- Conversions ----------

#[when(expr = "the float {word} is converted to int")]
fn float_to_int(w: &mut Steps, f: String) {
    let f: f64 = f.parse().unwrap();
    let v = Val::bytes(TypeId::FLOAT, f.to_bits().to_le_bytes().to_vec());
    w.error = convert(&v, TypeId::INT).unwrap_err().to_string();
}

/// The error text of a conversion, or the value it gave.
fn outcome(r: Result<Val, ultragraphdb_core_types::TypesError>) -> String {
    match r {
        Ok(v) => format!("{v:?}"),
        Err(e) => e.to_string(),
    }
}

#[when(
    expr = "the floats {string}, {string} and {string} and the bigfloat {string} are each converted to datetime"
)]
fn floats_to_datetime(w: &mut Steps, a: String, b: String, c: String, big: String) {
    for f in [a, b, c] {
        let f: f64 = f.parse().unwrap();
        let v = Val::bytes(TypeId::FLOAT, f.to_bits().to_le_bytes().to_vec());
        w.failures.push(outcome(convert(&v, TypeId::DATETIME)));
    }
    let v = Val::bytes(TypeId::BIGFLOAT, big.into_bytes());
    w.failures.push(outcome(convert(&v, TypeId::DATETIME)));
}

#[then(expr = "each conversion fails with {string}")]
fn each_conversion_fails(w: &mut Steps, msg: String) {
    assert_eq!(w.failures, vec![msg; 4]);
}

#[when(expr = "the binary value with the bytes {string} is converted to string")]
fn invalid_binary_to_string(w: &mut Steps, bytes: String) {
    w.error = outcome(convert(
        &Val::bytes(TypeId::BINARY, unhex(&bytes)),
        TypeId::STRING,
    ));
}

#[when(expr = "the binary value of {string} is converted to string")]
fn binary_to_string(w: &mut Steps, s: String) {
    w.text =
        text_of(&convert(&Val::bytes(TypeId::BINARY, s.into_bytes()), TypeId::STRING).unwrap());
}

#[when(expr = "the string {string} is converted to float")]
fn string_to_float(w: &mut Steps, s: String) {
    w.error = convert(&Val::bytes(TypeId::STRING, s.into_bytes()), TypeId::FLOAT)
        .unwrap_err()
        .to_string();
}

#[then(expr = "conversion fails with {string}")]
fn conversion_fails(w: &mut Steps, msg: String) {
    assert_eq!(w.error, msg);
}

#[when(expr = "the bytes [], [0], [1] and [2] are converted to bool")]
fn bytes_to_bool(w: &mut Steps) {
    w.bools = [vec![], vec![0], vec![1], vec![2]]
        .into_iter()
        .map(
            |b| match convert(&Val::bytes(TypeId::BINARY, b), TypeId::BOOL) {
                Ok(v) => match v.into_value() {
                    Some(Value::Bool(b)) => Ok(b),
                    other => Err(format!("{other:?}")),
                },
                Err(e) => Err(e.to_string()),
            },
        )
        .collect();
}

#[then(expr = "the results are false, false and true, and [2] fails")]
fn bools_match(w: &mut Steps) {
    assert_eq!(&w.bools[..3], &[Ok(false), Ok(false), Ok(true)]);
    assert_eq!(w.bools[3], Err("invalid value for bool 2".into()));
}

#[when(expr = "the float 2.5e-7 is converted to string")]
fn float_to_string(w: &mut Steps) {
    let v = Val::bytes(TypeId::FLOAT, 2.5e-7f64.to_bits().to_le_bytes().to_vec());
    w.text = text_of(&convert(&v, TypeId::STRING).unwrap());
}

#[then(expr = "the result is {string}")]
fn text_is(w: &mut Steps, s: String) {
    assert_eq!(w.text, s);
}

// ---------- Big floats ----------

#[derive(Debug, Clone, Deserialize)]
struct BigFloatCase {
    kind: String,
    #[serde(rename = "in")]
    input: String,
    #[serde(default)]
    err: String,
    #[serde(default)]
    p: String,
    #[serde(default)]
    text: String,
}

#[when(
    expr = "the golden big-float inputs are parsed and formatted: hand-picked, random, exact midpoints between adjacent 200-bit values, and powers of two"
)]
fn big_float_golden(w: &mut Steps) {
    let cases = golden::<BigFloatCase>("types_bigfloat");
    for kind in ["hand", "random", "midpoint", "pow2"] {
        assert!(cases.iter().any(|c| c.kind == kind), "no {kind} vectors");
    }
    for c in cases {
        match BigFloat::unmarshal_text(&c.input, BIG_FLOAT_PRECISION) {
            Err(e) if e == c.err => {}
            Ok(b)
                if c.err.is_empty()
                    && b.text_p() == c.p
                    && (c.kind == "extreme" || b.marshal_text() == c.text) => {}
            other => w.failures.push(format!(
                "{:?}: {other:?}, Go {} {} {}",
                c.input, c.p, c.text, c.err
            )),
        }
    }
}

#[then(expr = "every 200-bit value equals Go's and every text equals Go's MarshalText")]
fn big_floats_match(w: &mut Steps) {
    assert!(w.failures.is_empty(), "{}", w.failures.join("\n"));
}

#[when(expr = "2^-1100 at 200 bits is formatted")]
fn format_pow2(w: &mut Steps) {
    w.text = BigFloat::unmarshal_text("0x1p-1100", BIG_FLOAT_PRECISION)
        .unwrap()
        .marshal_text();
}

#[then(expr = "the text is {string}")]
fn the_text_is(w: &mut Steps, s: String) {
    assert_eq!(w.text, s);
}

#[when(expr = "{string}, {string}, {string} and {string} are parsed as bigfloat")]
fn parse_big_floats(w: &mut Steps, a: String, b: String, c: String, d: String) {
    w.texts = [a, b, c, d]
        .iter()
        .map(
            |s| match convert(&Val::bytes(TypeId::STRING, s.as_bytes()), TypeId::BIGFLOAT) {
                Ok(v) => match v.into_value() {
                    Some(Value::BigFloat(f)) => Ok(f.marshal_text()),
                    other => Err(format!("{other:?}")),
                },
                Err(e) => Err(e.to_string()),
            },
        )
        .collect();
}

#[then(expr = "the first three give {string}, {string} and {string}, and {string} is refused")]
fn big_float_syntax(w: &mut Steps, a: String, b: String, c: String, refused: String) {
    assert_eq!(&w.texts[..3], &[Ok(a), Ok(b), Ok(c)]);
    assert_eq!(
        w.texts[3],
        Err(format!(
            "math/big: cannot unmarshal \"{refused}\" into a *big.Float (number has no digits)"
        ))
    );
}

// ---------- Datetimes ----------

#[when(expr = "{string}, {string} and {string} are parsed")]
fn parse_dates(w: &mut Steps, a: String, b: String, c: String) {
    w.times = [a, b, c].iter().map(|s| parse_time(s).unwrap()).collect();
}

#[then(expr = "each yields midnight UTC on the first day of the period it names")]
fn midnight(w: &mut Steps) {
    let want = [
        DateTime::utc(2018, 1, 1, 0, 0, 0, 0),
        DateTime::utc(2018, 10, 1, 0, 0, 0, 0),
        DateTime::utc(2018, 10, 3, 0, 0, 0, 0),
    ];
    assert_eq!(w.times, want);
}

#[when(expr = "{string} is parsed")]
fn parse_one(w: &mut Steps, s: String) {
    w.error = parse_time(&s).unwrap_err().to_string();
}

#[then(expr = "parsing fails with {string}")]
fn parsing_fails(w: &mut Steps, msg: String) {
    assert!(w.error.contains(&msg), "{}", w.error);
}

// ---------- Inference ----------

#[when(
    expr = "{string}, {string}, {string}, {string}, {string}, {string} and {string} are inferred"
)]
#[allow(clippy::too_many_arguments)]
fn infer(
    w: &mut Steps,
    a: String,
    b: String,
    c: String,
    d: String,
    e: String,
    f: String,
    g: String,
) {
    w.types = [a, b, c, d, e, f, g]
        .iter()
        .map(|s| type_for_value(s.as_bytes()).0)
        .collect();
}

#[then(expr = "the types are bool, bool, default, int, datetime, default and float")]
fn inferred(w: &mut Steps) {
    use TypeId as T;
    assert_eq!(
        w.types,
        [
            T::BOOL,
            T::BOOL,
            T::DEFAULT,
            T::INT,
            T::DATETIME,
            T::DEFAULT,
            T::FLOAT
        ]
    );
}

// ---------- Passwords ----------

#[when(expr = "the string {string} is converted to password")]
fn to_password(w: &mut Steps, s: String) {
    w.error = convert(
        &Val::bytes(TypeId::STRING, s.into_bytes()),
        TypeId::PASSWORD,
    )
    .unwrap_err()
    .to_string();
}

#[derive(Debug, Clone, Deserialize)]
struct EncryptCase {
    plain: String,
    #[serde(default)]
    hash: String,
}

#[derive(Debug, Clone, Deserialize)]
struct PasswordFile {
    encrypt: Vec<EncryptCase>,
}

#[when(expr = "a golden password and its Go-generated bcrypt hash are checked")]
fn check_hash(w: &mut Steps) {
    let file = &golden::<PasswordFile>("types_password")[0];
    let c = file
        .encrypt
        .iter()
        .find(|c| !c.hash.is_empty())
        .expect("a Go-written hash");
    w.hash_ok = verify_password(&c.plain, &c.hash).is_ok();
    w.other_fails = verify_password(format!("{}x", c.plain), &c.hash).is_err();
    assert!(encrypt(&c.plain).is_ok_and(|h| h.starts_with("$2a$10$")));
}

#[then(expr = "the check succeeds, and a different password fails it")]
fn hash_checked(w: &mut Steps) {
    assert!(w.hash_ok && w.other_fails);
}

// ---------- Sorting ----------

#[when(expr = "the values 3, 2.5 and -1 of mixed int and float types are sorted")]
fn sort_mixed(w: &mut Steps) {
    let mut rows = vec![
        vec![Val::new(TypeId::INT, Value::Int(3))],
        vec![Val::new(TypeId::FLOAT, Value::Float(2.5))],
        vec![Val::new(TypeId::INT, Value::Int(-1))],
    ];
    let mut uids = vec![1, 2, 3];
    sort(&mut rows, &mut uids, &[false], "", &TextCollator).unwrap();
    w.sorted = rows.iter().map(|r| text_of(&r[0])).collect();
}

#[then(expr = "the order is -1, 2.5, 3")]
fn mixed_order(w: &mut Steps) {
    assert_eq!(w.sorted, ["-1", "2.5", "3"]);
}

#[derive(Debug, Clone, Deserialize)]
struct LangSortCase {
    lang: String,
    #[serde(rename = "in")]
    input: Vec<String>,
    out: Vec<String>,
}

#[given(expr = "the golden German strings")]
fn german_strings(w: &mut Steps) {
    w.golden_lang = golden::<LangSortCase>("types_sort_lang")
        .into_iter()
        .filter(|c| c.lang == "de")
        .collect();
    assert_eq!(w.golden_lang.len(), 1);
}

#[when(expr = "the golden German strings are sorted with language {string}")]
fn sort_german(w: &mut Steps, lang: String) {
    let c = &w.golden_lang[0];
    assert_eq!(c.lang, lang);
    let mut rows: Vec<Vec<Val>> = c
        .input
        .iter()
        .map(|s| vec![Val::new(TypeId::STRING, Value::str(s))])
        .collect();
    let mut uids: Vec<u64> = (1..=rows.len() as u64).map(|i| i * 100).collect();
    sort(&mut rows, &mut uids, &[false], &lang, &TextCollator).unwrap();
    w.sorted = rows.iter().map(|r| text_of(&r[0])).collect();
}

#[then(expr = "the order equals Dgraph's")]
fn german_order(w: &mut Steps) {
    assert_eq!(
        w.sorted, w.golden_lang[0].out,
        "input {:?}",
        w.golden_lang[0].input
    );
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_test_bdd::features_dir()),
    );
}
