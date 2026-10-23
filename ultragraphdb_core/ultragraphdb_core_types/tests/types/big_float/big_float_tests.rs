/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Go's math/big parse and format at 200 bits, against the golden vectors (tools/golden/types_bigfloat.go).

use std::cmp::Ordering;
use std::collections::BTreeMap;

use serde::Deserialize;
use ultragraphdb_core_types::{BIG_FLOAT_PRECISION, BigFloat, TypeId, Val, Value, convert};

#[derive(Deserialize)]
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
    #[serde(default)]
    str: String,
}

fn str_of(v: Val) -> String {
    match v.into_value() {
        Some(Value::Str(b)) | Some(Value::Bytes(b)) => String::from_utf8(b).unwrap(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_golden_big_floats() {
    let path = ultragraphdb_test_bdd::testdata_dir().join("golden/types_bigfloat.json");
    let golden = ultragraphdb_test_bdd::load_golden::<BigFloatCase>(&path);
    // (kind) -> (cases, mismatches)
    let mut stats: BTreeMap<&str, (usize, Vec<String>)> = BTreeMap::new();
    for c in golden.cases() {
        let e = stats.entry(c.kind.as_str()).or_default();
        e.0 += 1;
        let parsed = convert(
            &Val::bytes(TypeId::STRING, c.input.as_bytes()),
            TypeId::BIGFLOAT,
        );
        let got = match parsed {
            Err(err) => {
                if err.to_string() != c.err {
                    e.1.push(format!("{:?}: error {err}, Go {:?}", c.input, c.err));
                }
                continue;
            }
            Ok(v) => match v.into_value() {
                Some(Value::BigFloat(b)) => b,
                other => panic!("{other:?}"),
            },
        };
        if got.text_p() != c.p || got.prec() != BIG_FLOAT_PRECISION {
            e.1.push(format!(
                "{:?}: value {} at {}, Go {}",
                c.input,
                got.text_p(),
                got.prec(),
                c.p
            ));
            continue;
        }
        if c.kind == "extreme" {
            continue;
        }
        let text = str_of(
            convert(
                &Val::bytes(TypeId::BIGFLOAT, c.input.as_bytes()),
                TypeId::BINARY,
            )
            .unwrap(),
        );
        let s = str_of(
            convert(
                &Val::bytes(TypeId::BIGFLOAT, c.input.as_bytes()),
                TypeId::STRING,
            )
            .unwrap(),
        );
        if text != c.text || s != c.str {
            e.1.push(format!(
                "{:?}: text {text} / {s}, Go {} / {}",
                c.input, c.text, c.str
            ));
        }
    }
    let report: Vec<String> = stats
        .iter()
        .filter(|(_, (_, bad))| !bad.is_empty())
        .map(|(k, (n, bad))| {
            format!(
                "{k}: {} of {n} differ, e.g.\n  {}",
                bad.len(),
                bad[..bad.len().min(5)].join("\n  ")
            )
        })
        .collect();
    assert!(report.is_empty(), "{}", report.join("\n"));
}

#[test]
fn test_power_of_two_formats_as_go_shortens_it() {
    let x = BigFloat::unmarshal_text("0x1p-1100", BIG_FLOAT_PRECISION).unwrap();
    assert_eq!(
        x.marshal_text(),
        "7.3621518290228626754368661771449651176491350325096363488672e-332"
    );
}

#[test]
fn test_conversions() {
    let x = BigFloat::unmarshal_text("-2.5", BIG_FLOAT_PRECISION).unwrap();
    assert_eq!(x.to_f64(), -2.5);
    assert_eq!(x.to_i64(), -2);
    assert_eq!(x.text_g(10), "-2.5");
    assert_eq!(BigFloat::from_i64(i64::MIN, 200).to_i64(), i64::MIN);
    assert_eq!(BigFloat::inf(false, 200).to_i64(), i64::MAX);
    assert_eq!(BigFloat::from_f64(5e-324, 200).unwrap().to_f64(), 5e-324);
    assert!(BigFloat::from_f64(f64::NAN, 200).is_none());
    let one = BigFloat::from_i64(1, 200);
    assert_eq!(one.compare(&BigFloat::from_i64(2, 200)), Ordering::Less);
    assert_eq!(
        BigFloat::zero(200).compare(&BigFloat::from_f64(-0.0, 200).unwrap()),
        Ordering::Equal
    );
    assert_eq!(
        one.quo(&BigFloat::from_i64(3, 200), 53).unwrap().text_g(-1),
        "0.3333333333333333"
    );
}

#[test]
fn test_unmarshal_errors() {
    assert_eq!(
        BigFloat::unmarshal_text("NaN", 200).unwrap_err(),
        "math/big: cannot unmarshal \"NaN\" into a *big.Float (number has no digits)"
    );
    assert_eq!(
        BigFloat::unmarshal_text("", 200).unwrap_err(),
        "math/big: cannot unmarshal \"\" into a *big.Float (EOF)"
    );
}

#[test]
fn test_quo_of_zero_by_zero_or_inf_by_inf_is_nan() {
    // Go's Quo panics with ErrNaN.
    let (zero, inf) = (BigFloat::zero(53), BigFloat::inf(false, 53));
    assert_eq!(zero.quo(&zero, 53), None);
    assert_eq!(inf.quo(&inf, 53), None);
    assert_eq!(zero.quo(&inf, 53).map(|z| z.is_zero()), Some(true));
    assert_eq!(inf.quo(&zero, 53).map(|z| z.is_inf()), Some(true));
}

#[test]
fn test_a_precision_of_zero_is_gos_default() {
    // Go gives SetInt64 and Parse 64 bits and SetFloat64 53 at precision 0; the port divided by a zero mantissa.
    let five = BigFloat::from_i64(5, 0);
    assert_eq!(five.prec(), 64);
    assert_eq!(BigFloat::from_f64(0.5, 0).unwrap().prec(), 53);
    let two = BigFloat::unmarshal_text("2", 0).unwrap();
    assert_eq!(two.prec(), 64);
    let q = five.quo(&two, 0).unwrap();
    assert_eq!((q.prec(), q.text_g(-1)), (64, "2.5".to_string()));
    assert!(BigFloat::unmarshal_text("1e1", u32::MAX).is_ok());
}

#[test]
fn test_an_exponent_past_int64_is_an_overflow() {
    // Go's int64 sums wrap here; the port's overflowed.
    for text in [
        "1.5e-9223372036854775808",
        "1.5e9223372036854775807",
        "0x1.8p-9223372036854775808",
    ] {
        let err = BigFloat::unmarshal_text(text, 200).unwrap_err();
        assert!(err.ends_with("(exponent overflow)"), "{text}: {err}");
    }
}
