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
        Some(Value::Str(s)) => s,
        Some(Value::Bytes(b)) => String::from_utf8(b).unwrap(),
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
    assert_eq!(BigFloat::from_i64(i64::MAX, 200).to_i64(), i64::MAX);
    assert_eq!(BigFloat::inf(false, 200).to_i64(), i64::MAX);
    assert_eq!(BigFloat::inf(true, 200).to_i64(), i64::MIN);
    assert_eq!(BigFloat::from_f64(5e-324, 200).unwrap().to_f64(), 5e-324);
    assert!(BigFloat::from_f64(f64::NAN, 200).is_none());
    assert_eq!(
        BigFloat::from_f64(f64::INFINITY, 200),
        Some(BigFloat::inf(false, 200))
    );
    assert_eq!(
        BigFloat::from_f64(f64::NEG_INFINITY, 200),
        Some(BigFloat::inf(true, 200))
    );
    let neg_zero = BigFloat::from_f64(-0.0, 200).unwrap();
    assert!(neg_zero.is_zero() && neg_zero.is_neg() && !neg_zero.is_inf());
    assert!(neg_zero.to_f64().is_sign_negative() && neg_zero.to_f64() == 0.0);
    assert!(!BigFloat::zero(200).is_neg());
    // SetInt64 and SetFloat64 are exact at their own widths and round to nearest even below them.
    assert_eq!(
        BigFloat::from_i64(i64::MIN, 200).text_g(-1),
        "-9.223372036854775808e+18"
    );
    assert_eq!(BigFloat::from_i64((1 << 53) + 1, 53).to_i64(), 1 << 53);
    assert_eq!(
        BigFloat::from_i64((1 << 53) + 3, 53).to_i64(),
        (1 << 53) + 4
    );
    assert_eq!(
        BigFloat::from_f64(f64::MAX, 200).unwrap().to_f64(),
        f64::MAX
    );
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

/// Go 1.26's `Text('g', p)` for p in -1, 0, 1, 2, 5, 10 and 20, `Float64` and `Int64` of each value at 200 bits.
#[test]
fn test_text_g_float64_and_int64_as_go_writes_them() {
    #[rustfmt::skip]
    let cases: [(&str, [&str; 7], f64, i64); 16] = [
        ("0", ["0", "0", "0", "0", "0", "0", "0"], 0.0, 0),
        ("-0", ["-0", "-0", "-0", "-0", "-0", "-0", "-0"], -0.0, 0),
        ("1", ["1", "1", "1", "1", "1", "1", "1"], 1.0, 1),
        ("-2.5", ["-2.5", "-2", "-2", "-2.5", "-2.5", "-2.5", "-2.5"], -2.5, -2),
        ("9.5", ["9.5", "1e+01", "1e+01", "9.5", "9.5", "9.5", "9.5"], 9.5, 9),
        ("0.5", ["0.5", "0.5", "0.5", "0.5", "0.5", "0.5", "0.5"], 0.5, 0),
        ("123456789", ["1.23456789e+08", "1e+08", "1e+08", "1.2e+08", "1.2346e+08", "123456789", "123456789"], 1.23456789e+08, 123456789),
        ("1e-7", ["1e-07", "1e-07", "1e-07", "1e-07", "1e-07", "1e-07", "1e-07"], 1e-07, 0),
        ("1e21", ["1e+21", "1e+21", "1e+21", "1e+21", "1e+21", "1e+21", "1e+21"], 1e+21, i64::MAX),
        ("0.000123456", ["0.000123456", "0.0001", "0.0001", "0.00012", "0.00012346", "0.000123456", "0.000123456"], 0.000123456, 0),
        ("3.14159265358979323846264338327950288", ["3.14159265358979323846264338327950288", "3", "3", "3.1", "3.1416", "3.141592654", "3.1415926535897932385"], std::f64::consts::PI, 3),
        ("Inf", ["+Inf", "+Inf", "+Inf", "+Inf", "+Inf", "+Inf", "+Inf"], f64::INFINITY, i64::MAX),
        ("-Inf", ["-Inf", "-Inf", "-Inf", "-Inf", "-Inf", "-Inf", "-Inf"], f64::NEG_INFINITY, i64::MIN),
        ("1e100", ["1e+100", "1e+100", "1e+100", "1e+100", "1e+100", "1e+100", "1e+100"], 1e+100, i64::MAX),
        ("99999.5", ["99999.5", "1e+05", "1e+05", "1e+05", "1e+05", "99999.5", "99999.5"], 99999.5, 99999),
        ("-0.00001", ["-1e-05", "-1e-05", "-1e-05", "-1e-05", "-1e-05", "-1e-05", "-1e-05"], -1e-05, 0),
    ];
    for (input, texts, f, i) in cases {
        let x = BigFloat::unmarshal_text(input, BIG_FLOAT_PRECISION).unwrap();
        assert_eq!(x.prec(), BIG_FLOAT_PRECISION);
        for (p, want) in [-1, 0, 1, 2, 5, 10, 20].into_iter().zip(texts) {
            assert_eq!(x.text_g(p), want, "{input} at {p}");
        }
        assert_eq!(x.marshal_text(), texts[0], "{input}");
        assert_eq!(x.to_f64().to_bits(), f.to_bits(), "{input}");
        assert_eq!(x.to_i64(), i, "{input}");
    }
}

/// Go 1.26's `new(Float).SetPrec(p).Quo(x, y)` of int64 operands, as `Text('g', -1)` writes it.
#[test]
fn test_quo_rounds_as_go_does() {
    let cases: [((i64, i64), [&str; 5]); 6] = [
        (
            (1, 3),
            [
                "0.33333333333333333334",
                "0.2",
                "0.33333334",
                "0.3333333333333333",
                "0.3333333333333333333333333333333333333333333333333333333333334",
            ],
        ),
        (
            (2, 3),
            [
                "0.6666666666666666667",
                "0.5",
                "0.6666667",
                "0.6666666666666666",
                "0.666666666666666666666666666666666666666666666666666666666667",
            ],
        ),
        (
            (-1, 7),
            [
                "-0.14285714285714285714",
                "-0.1",
                "-0.14285715",
                "-0.14285714285714285",
                "-0.1428571428571428571428571428571428571428571428571428571428571",
            ],
        ),
        ((10, 4), ["2.5", "2", "2.5", "2.5", "2.5"]),
        (
            (1, 1 << 62),
            [
                "2.168404344971008868e-19",
                "2e-19",
                "2.1684043e-19",
                "2.168404344971009e-19",
                "2.1684043449710088680149056017398834228515625e-19",
            ],
        ),
        ((7, -2), ["-3.5", "-4", "-3.5", "-3.5", "-3.5"]),
    ];
    for ((a, b), texts) in cases {
        let (x, y) = (BigFloat::from_i64(a, 0), BigFloat::from_i64(b, 0));
        for (p, want) in [0u32, 1, 24, 53, 200].into_iter().zip(texts) {
            let z = x.quo(&y, p).unwrap();
            // A precision of 0 is the operands' 64 bits.
            assert_eq!(z.prec(), if p == 0 { 64 } else { p });
            assert_eq!(z.text_g(-1), want, "{a}/{b} at {p}");
        }
    }
}

/// Go's TestFloatQuoSmoke (math/big/float_test.go): small integer quotients at 53 bits are float64's.
#[test]
fn test_quo_smoke() {
    const DPREC: i32 = 3;
    const PREC: i32 = 10 + DPREC;
    for x in -10..=10 {
        for y in -10..10 {
            if y == 0 {
                continue;
            }
            let (a, b) = (f64::from(x), f64::from(y));
            for ad in -DPREC..=DPREC {
                for bd in -DPREC..=DPREC {
                    let fa = BigFloat::from_f64(a, (PREC + ad) as u32).unwrap();
                    let fb = BigFloat::from_f64(b, (PREC + bd) as u32).unwrap();
                    let c = fa.quo(&fb, 53).unwrap();
                    assert_eq!(c.to_f64().to_bits(), (a / b).to_bits(), "{a}/{b}");
                }
            }
        }
    }
}

/// Go's TestFloatCmpSpecialValues: Cmp orders ±0, finite values and ±Inf as float64 does.
#[test]
#[allow(clippy::approx_constant, reason = "2.71828 is Go's test value, not e")]
fn test_compare_special_values() {
    let args = [
        f64::NEG_INFINITY,
        -2.71828,
        -1.0,
        -0.0,
        0.0,
        1.0,
        2.71828,
        f64::INFINITY,
    ];
    for x in args {
        for y in args {
            let (bx, by) = (
                BigFloat::from_f64(x, 53).unwrap(),
                BigFloat::from_f64(y, 53).unwrap(),
            );
            assert_eq!(bx.compare(&by), x.partial_cmp(&y).unwrap(), "{x} {y}");
        }
    }
    // Magnitudes that differ only below the 53rd bit, and opposite signs of one magnitude.
    let a = BigFloat::unmarshal_text("1.0000000000000000000000000000000000000001", 200).unwrap();
    let b = BigFloat::unmarshal_text("1", 200).unwrap();
    assert_eq!(
        (a.compare(&b), b.compare(&a)),
        (Ordering::Greater, Ordering::Less)
    );
    let (na, nb) = (
        BigFloat::unmarshal_text("-1.0000000000000000000000000000000000000001", 200).unwrap(),
        BigFloat::unmarshal_text("-1", 200).unwrap(),
    );
    assert_eq!(
        (na.compare(&nb), nb.compare(&na)),
        (Ordering::Less, Ordering::Greater)
    );
    assert_eq!(na.compare(&a), Ordering::Less);
    assert_eq!(a.compare(&a.clone()), Ordering::Equal);
}

/// Go's TestFloatInt64 (math/big/float_test.go), each value parsed at 1000 bits as its makeFloat does.
#[test]
fn test_go_float_int64() {
    let cases: [(&str, i64); 25] = [
        ("-Inf", i64::MIN),
        ("-1e10000", i64::MIN),
        ("-9223372036854775809", i64::MIN),
        ("-9223372036854775808.000000000000000000001", i64::MIN),
        ("-9223372036854775808", -9223372036854775808),
        (
            "-9223372036854775807.000000000000000000001",
            -9223372036854775807,
        ),
        ("-9223372036854775807", -9223372036854775807),
        ("-12345.000000000000000000001", -12345),
        ("-12345.0", -12345),
        ("-1.000000000000000000001", -1),
        ("-1.5", -1),
        ("-1", -1),
        ("-1e-1000", 0),
        ("0", 0),
        ("1e-1000", 0),
        ("1", 1),
        ("1.000000000000000000001", 1),
        ("1.5", 1),
        ("12345.0", 12345),
        ("12345.000000000000000000001", 12345),
        ("9223372036854775807", 9223372036854775807),
        ("9223372036854775807.000000000000000000001", i64::MAX),
        ("9223372036854775808", i64::MAX),
        ("1e10000", i64::MAX),
        ("+Inf", i64::MAX),
    ];
    for (s, want) in cases {
        let x = BigFloat::unmarshal_text(s, 1000).unwrap();
        assert_eq!(x.to_i64(), want, "{s}");
    }
}

/// Go's TestFloatFloat64 (math/big/float_test.go), each value and its negation parsed at 1000 bits.
#[test]
fn test_go_float_float64() {
    const SMALLEST_NORMAL: f64 = 2.2250738585072014e-308;
    let denormal = f64::from_bits;
    let cases: [(&str, f64); 49] = [
        ("0", 0.0),
        // underflow to zero
        ("1e-1000", 0.0),
        ("0x0.0000000000001p-1023", 0.0),
        ("0x0.00000000000008p-1022", 0.0),
        // denormals
        ("0x0.0000000000000cp-1022", f64::from_bits(1)),
        ("0x0.00000000000010p-1022", f64::from_bits(1)),
        ("0x.8p-1073", f64::from_bits(1)),
        ("1p-1074", f64::from_bits(1)),
        ("0x.fffffffffffffp-1022", denormal(0x000fffffffffffff)),
        // special denormal cases (Go issues 14553, 14651)
        ("0x0.00000000000001p-1022", denormal(0)),
        ("0x0.00000000000004p-1022", denormal(0)),
        ("0x0.00000000000008p-1022", denormal(0)),
        ("0x0.00000000000009p-1022", denormal(1)),
        ("0x0.0000000000000ap-1022", denormal(1)),
        ("0x0.8p-1074", denormal(0)),
        ("0x0.9p-1074", denormal(1)),
        ("0x0.ap-1074", denormal(1)),
        ("0x0.bp-1074", denormal(1)),
        ("0x0.cp-1074", denormal(1)),
        ("0x1.0p-1074", denormal(1)),
        ("0x1.7p-1074", denormal(1)),
        ("0x1.8p-1074", denormal(2)),
        ("0x1.9p-1074", denormal(2)),
        ("0x2.0p-1074", denormal(2)),
        ("0x2.8p-1074", denormal(2)),
        ("0x2.9p-1074", denormal(3)),
        ("0x3.0p-1074", denormal(3)),
        ("0x3.7p-1074", denormal(3)),
        ("0x3.8p-1074", denormal(4)),
        ("0x4.0p-1074", denormal(4)),
        ("0x4.8p-1074", denormal(4)),
        ("0x4.9p-1074", denormal(5)),
        // normals
        (
            "0x.fffffffffffff8p-1022",
            f64::from_bits(0x0010000000000000),
        ),
        ("1p-1022", f64::from_bits(0x0010000000000000)),
        ("1", 1.0),
        ("1.000000000000000000001", 1.0),
        ("12345.0", 12345.0),
        ("12345.000000000000000000001", 12345.0),
        ("0x1.fffffffffffff0p1023", f64::MAX),
        ("0x1.fffffffffffff4p1023", f64::MAX),
        // overflow
        ("0x1.fffffffffffff8p1023", f64::INFINITY),
        ("0x1p1024", f64::INFINITY),
        ("1e10000", f64::INFINITY),
        ("0x1.fffffffffffff8p2147483646", f64::INFINITY),
        ("Inf", f64::INFINITY),
        // selected denormalized values that were handled incorrectly in the past
        (
            "0x.fffffffffffffp-1022",
            SMALLEST_NORMAL - f64::from_bits(1),
        ),
        (
            "4503599627370495p-1074",
            SMALLEST_NORMAL - f64::from_bits(1),
        ),
        ("2.2250738585072011e-308", 2.225073858507201e-308),
        ("2.2250738585072012e-308", 2.2250738585072014e-308),
    ];
    for (s, want) in cases {
        for (text, want) in [(s.to_string(), want), (format!("-{s}"), -want)] {
            let x = BigFloat::unmarshal_text(&text, 1000).unwrap();
            let got = x.to_f64();
            assert_eq!(
                got.to_bits(),
                want.to_bits(),
                "{text}: {got:e}, want {want:e}"
            );
            // SetFloat64(f).Float64() is f.
            assert_eq!(
                BigFloat::from_f64(got, 0).unwrap().to_f64().to_bits(),
                got.to_bits(),
                "{text}"
            );
        }
    }
}

#[test]
fn test_unmarshal_errors() {
    // Go 1.26's UnmarshalText at 200 bits: the reason each text is refused.
    let refused = [
        ("NaN", "number has no digits"),
        ("", "EOF"),
        ("1e", "number has no digits"),
        ("0x", "number has no digits"),
        ("1.2.3", "expected end of string, found '.'"),
        (" 1", "number has no digits"),
        ("1 ", "expected end of string, found ' '"),
        ("+-1", "number has no digits"),
        ("0b102", "expected end of string, found '2'"),
        ("1__0", "'_' must separate successive digits"),
        ("_1", "'_' must separate successive digits"),
        ("0x1p", "number has no digits"),
        ("infinity", "number has no digits"),
        ("Inf ", "number has no digits"),
        ("INF", "number has no digits"),
        ("1e+", "number has no digits"),
        ("--1", "number has no digits"),
        ("-", "number has no digits"),
        ("+", "number has no digits"),
        (".", "number has no digits"),
        ("0x.p1", "number has no digits"),
        ("1e99999999999", "exponent overflow"),
        ("1e-99999999999", "exponent overflow"),
        ("日本", "number has no digits"),
    ];
    for (text, reason) in refused {
        assert_eq!(
            BigFloat::unmarshal_text(text, 200).unwrap_err(),
            format!("math/big: cannot unmarshal {text:?} into a *big.Float ({reason})")
        );
    }
    // And what it accepts, as Text('g', -1) writes it.
    let accepted = [
        ("0b1.1", "1.5"),
        ("0o17", "15"),
        ("017", "17"),
        ("0x1.8p1", "3"),
        ("1_000", "1000"),
        ("+Inf", "+Inf"),
        ("-inf", "-Inf"),
        ("1E5", "100000"),
        ("0X1P4", "16"),
        ("1p4", "16"),
    ];
    for (text, want) in accepted {
        assert_eq!(
            BigFloat::unmarshal_text(text, 200).map(|x| x.marshal_text()),
            Ok(want.to_string()),
            "{text}"
        );
    }
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
    let ten = BigFloat::unmarshal_text("1e1", u32::MAX).unwrap();
    assert_eq!((ten.prec(), ten.to_i64()), (u32::MAX, 10));
}

#[test]
fn test_an_exponent_past_int64_is_an_overflow() {
    // Go's int64 sums wrap here; the port's overflowed.
    for text in [
        "1.5e-9223372036854775808",
        "1.5e9223372036854775807",
        "0x1.8p-9223372036854775808",
    ] {
        assert_eq!(
            BigFloat::unmarshal_text(text, 200).unwrap_err(),
            format!("math/big: cannot unmarshal {text:?} into a *big.Float (exponent overflow)")
        );
    }
}
