/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Expected values are Go 1.26's, printed by the strconv functions themselves.

use ultragraphdb_common_utils::go_strconv;

/// An input and Go's result under two settings: two bases, or two bit sizes.
type TwoWays<T> = (
    &'static [u8],
    Result<T, &'static str>,
    Result<T, &'static str>,
);

fn text<T>(r: Result<T, ultragraphdb_error_num::NumError>) -> Result<T, String> {
    r.map_err(|e| e.to_string())
}

#[test]
fn test_parse_int_base_10() {
    let cases: [(&[u8], Result<i64, &str>); 15] = [
        (b"0", Ok(0)),
        (b"-0", Ok(0)),
        (b"+7", Ok(7)),
        (b"-9223372036854775808", Ok(-9223372036854775808)),
        (b"9223372036854775807", Ok(9223372036854775807)),
        (
            b"9223372036854775808",
            Err("strconv.ParseInt: parsing \"9223372036854775808\": value out of range"),
        ),
        (
            b"-9223372036854775809",
            Err("strconv.ParseInt: parsing \"-9223372036854775809\": value out of range"),
        ),
        (b"", Err("strconv.ParseInt: parsing \"\": invalid syntax")),
        (b"+", Err("strconv.ParseInt: parsing \"+\": invalid syntax")),
        (b"-", Err("strconv.ParseInt: parsing \"-\": invalid syntax")),
        (
            b"1_000",
            Err("strconv.ParseInt: parsing \"1_000\": invalid syntax"),
        ),
        (
            b"12a",
            Err("strconv.ParseInt: parsing \"12a\": invalid syntax"),
        ),
        (
            b"99999999999999999999x",
            Err("strconv.ParseInt: parsing \"99999999999999999999x\": value out of range"),
        ),
        (
            b"0x10",
            Err("strconv.ParseInt: parsing \"0x10\": invalid syntax"),
        ),
        (
            b"\xff1",
            Err("strconv.ParseInt: parsing \"\\xff1\": invalid syntax"),
        ),
    ];
    for (s, want) in cases {
        assert_eq!(
            text(go_strconv::parse_int(s, 10)),
            want.map_err(str::to_string),
            "{s:?}"
        );
    }
}

#[test]
fn test_parse_int_base_0() {
    let cases: [(&[u8], Result<i64, &str>); 13] = [
        (b"0x1f", Ok(31)),
        (b"0X1F", Ok(31)),
        (b"0b101", Ok(5)),
        (b"0o17", Ok(15)),
        (b"017", Ok(15)),
        (b"0", Ok(0)),
        (
            b"0x",
            Err("strconv.ParseInt: parsing \"0x\": invalid syntax"),
        ),
        (b"1_000", Ok(1000)),
        (
            b"_1",
            Err("strconv.ParseInt: parsing \"_1\": invalid syntax"),
        ),
        (
            b"1__0",
            Err("strconv.ParseInt: parsing \"1__0\": invalid syntax"),
        ),
        (b"-0x8000000000000000", Ok(-9223372036854775808)),
        (
            b"0x8000000000000000",
            Err("strconv.ParseInt: parsing \"0x8000000000000000\": value out of range"),
        ),
        (
            b"08",
            Err("strconv.ParseInt: parsing \"08\": invalid syntax"),
        ),
    ];
    for (s, want) in cases {
        assert_eq!(
            text(go_strconv::parse_int(s, 0)),
            want.map_err(str::to_string),
            "{s:?}"
        );
    }
}

#[test]
fn test_parse_uint_base_0_and_10() {
    let cases: [TwoWays<u64>; 7] = [
        (
            b"18446744073709551615",
            Ok(18446744073709551615),
            Ok(18446744073709551615),
        ),
        (
            b"18446744073709551616",
            Err("strconv.ParseUint: parsing \"18446744073709551616\": value out of range"),
            Err("strconv.ParseUint: parsing \"18446744073709551616\": value out of range"),
        ),
        (
            b"-1",
            Err("strconv.ParseUint: parsing \"-1\": invalid syntax"),
            Err("strconv.ParseUint: parsing \"-1\": invalid syntax"),
        ),
        (
            b"+1",
            Err("strconv.ParseUint: parsing \"+1\": invalid syntax"),
            Err("strconv.ParseUint: parsing \"+1\": invalid syntax"),
        ),
        (
            b"0x_1f",
            Ok(31),
            Err("strconv.ParseUint: parsing \"0x_1f\": invalid syntax"),
        ),
        (
            b"0xffffffffffffffff",
            Ok(18446744073709551615),
            Err("strconv.ParseUint: parsing \"0xffffffffffffffff\": invalid syntax"),
        ),
        (
            b"0x10000000000000000",
            Err("strconv.ParseUint: parsing \"0x10000000000000000\": value out of range"),
            Err("strconv.ParseUint: parsing \"0x10000000000000000\": invalid syntax"),
        ),
    ];
    for (s, base0, base10) in cases {
        assert_eq!(
            text(go_strconv::parse_uint(s, 0)),
            base0.map_err(str::to_string),
            "{s:?}"
        );
        assert_eq!(
            text(go_strconv::parse_uint(s, 10)),
            base10.map_err(str::to_string),
            "{s:?}"
        );
    }
}

#[test]
fn test_atoi() {
    let cases: [(&[u8], Result<i64, &str>); 6] = [
        (b"42", Ok(42)),
        (b"-42", Ok(-42)),
        (b"+", Err("strconv.Atoi: parsing \"+\": invalid syntax")),
        (
            b"9223372036854775808",
            Err("strconv.Atoi: parsing \"9223372036854775808\": value out of range"),
        ),
        (b"1_0", Err("strconv.Atoi: parsing \"1_0\": invalid syntax")),
        (b"", Err("strconv.Atoi: parsing \"\": invalid syntax")),
    ];
    for (s, want) in cases {
        assert_eq!(
            text(go_strconv::atoi(s)),
            want.map_err(str::to_string),
            "{s:?}"
        );
    }
}

#[test]
fn test_parse_and_format_bool() {
    let cases: [(&[u8], Result<bool, &str>); 15] = [
        (b"1", Ok(true)),
        (b"t", Ok(true)),
        (b"T", Ok(true)),
        (b"TRUE", Ok(true)),
        (b"true", Ok(true)),
        (b"True", Ok(true)),
        (b"0", Ok(false)),
        (b"f", Ok(false)),
        (b"F", Ok(false)),
        (b"FALSE", Ok(false)),
        (b"false", Ok(false)),
        (b"False", Ok(false)),
        (
            b"yes",
            Err("strconv.ParseBool: parsing \"yes\": invalid syntax"),
        ),
        (
            b"tRUE",
            Err("strconv.ParseBool: parsing \"tRUE\": invalid syntax"),
        ),
        (b"", Err("strconv.ParseBool: parsing \"\": invalid syntax")),
    ];
    for (s, want) in cases {
        assert_eq!(
            text(go_strconv::parse_bool(s)),
            want.map_err(str::to_string),
            "{s:?}"
        );
    }
    assert_eq!(go_strconv::format_bool(true), "true");
    assert_eq!(go_strconv::format_bool(false), "false");
}

#[test]
fn test_parse_float_64_and_32() {
    let cases: [TwoWays<f64>; 33] = [
        (
            b"1.5",
            Ok(f64::from_bits(0x3ff8000000000000)),
            Ok(f64::from_bits(0x3ff8000000000000)),
        ),
        (
            b"-2.5e3",
            Ok(f64::from_bits(0xc0a3880000000000)),
            Ok(f64::from_bits(0xc0a3880000000000)),
        ),
        (
            b"1e400",
            Err("strconv.ParseFloat: parsing \"1e400\": value out of range"),
            Err("strconv.ParseFloat: parsing \"1e400\": value out of range"),
        ),
        (
            b"1e39",
            Ok(f64::from_bits(0x48078287f49c4a1d)),
            Err("strconv.ParseFloat: parsing \"1e39\": value out of range"),
        ),
        (
            b"0x1p-2",
            Ok(f64::from_bits(0x3fd0000000000000)),
            Ok(f64::from_bits(0x3fd0000000000000)),
        ),
        (
            b"0x1.8p1",
            Ok(f64::from_bits(0x4008000000000000)),
            Ok(f64::from_bits(0x4008000000000000)),
        ),
        (
            b"0X1P+3",
            Ok(f64::from_bits(0x4020000000000000)),
            Ok(f64::from_bits(0x4020000000000000)),
        ),
        (
            b"0x1p",
            Err("strconv.ParseFloat: parsing \"0x1p\": invalid syntax"),
            Err("strconv.ParseFloat: parsing \"0x1p\": invalid syntax"),
        ),
        (
            b"1_000.5",
            Ok(f64::from_bits(0x408f440000000000)),
            Ok(f64::from_bits(0x408f440000000000)),
        ),
        (
            b"1__0",
            Err("strconv.ParseFloat: parsing \"1__0\": invalid syntax"),
            Err("strconv.ParseFloat: parsing \"1__0\": invalid syntax"),
        ),
        (
            b"inf",
            Ok(f64::from_bits(0x7ff0000000000000)),
            Ok(f64::from_bits(0x7ff0000000000000)),
        ),
        (
            b"+Inf",
            Ok(f64::from_bits(0x7ff0000000000000)),
            Ok(f64::from_bits(0x7ff0000000000000)),
        ),
        (
            b"-infinity",
            Ok(f64::from_bits(0xfff0000000000000)),
            Ok(f64::from_bits(0xfff0000000000000)),
        ),
        (
            b"nan",
            Ok(f64::from_bits(0x7ff8000000000001)),
            Ok(f64::from_bits(0x7ff8000000000000)),
        ),
        (
            b"NaN",
            Ok(f64::from_bits(0x7ff8000000000001)),
            Ok(f64::from_bits(0x7ff8000000000000)),
        ),
        (
            b"+nan",
            Err("strconv.ParseFloat: parsing \"+nan\": invalid syntax"),
            Err("strconv.ParseFloat: parsing \"+nan\": invalid syntax"),
        ),
        (
            b"infx",
            Err("strconv.ParseFloat: parsing \"infx\": invalid syntax"),
            Err("strconv.ParseFloat: parsing \"infx\": invalid syntax"),
        ),
        (
            b".5",
            Ok(f64::from_bits(0x3fe0000000000000)),
            Ok(f64::from_bits(0x3fe0000000000000)),
        ),
        (
            b"5.",
            Ok(f64::from_bits(0x4014000000000000)),
            Ok(f64::from_bits(0x4014000000000000)),
        ),
        (
            b".",
            Err("strconv.ParseFloat: parsing \".\": invalid syntax"),
            Err("strconv.ParseFloat: parsing \".\": invalid syntax"),
        ),
        (
            b"1e",
            Err("strconv.ParseFloat: parsing \"1e\": invalid syntax"),
            Err("strconv.ParseFloat: parsing \"1e\": invalid syntax"),
        ),
        (
            b"1e+",
            Err("strconv.ParseFloat: parsing \"1e+\": invalid syntax"),
            Err("strconv.ParseFloat: parsing \"1e+\": invalid syntax"),
        ),
        (
            b"x",
            Err("strconv.ParseFloat: parsing \"x\": invalid syntax"),
            Err("strconv.ParseFloat: parsing \"x\": invalid syntax"),
        ),
        (
            b"",
            Err("strconv.ParseFloat: parsing \"\": invalid syntax"),
            Err("strconv.ParseFloat: parsing \"\": invalid syntax"),
        ),
        (
            b"4.9e-324",
            Ok(f64::from_bits(0x1)),
            Ok(f64::from_bits(0x0)),
        ),
        (b"1e-400", Ok(f64::from_bits(0x0)), Ok(f64::from_bits(0x0))),
        (
            b"0.1",
            Ok(f64::from_bits(0x3fb999999999999a)),
            Ok(f64::from_bits(0x3fb99999a0000000)),
        ),
        (
            b"3.4028235e38",
            Ok(f64::from_bits(0x47efffffe54daff8)),
            Ok(f64::from_bits(0x47efffffe0000000)),
        ),
        (
            b"3.4028236e38",
            Ok(f64::from_bits(0x47effffff514a7bc)),
            Err("strconv.ParseFloat: parsing \"3.4028236e38\": value out of range"),
        ),
        (
            b"123456789012345678901234567890",
            Ok(f64::from_bits(0x45f8ee90ff6c373e)),
            Ok(f64::from_bits(0x45f8ee9100000000)),
        ),
        (
            b"0x1.fffffffffffff8p1023",
            Err("strconv.ParseFloat: parsing \"0x1.fffffffffffff8p1023\": value out of range"),
            Err("strconv.ParseFloat: parsing \"0x1.fffffffffffff8p1023\": value out of range"),
        ),
        (
            b"0x1p1024",
            Err("strconv.ParseFloat: parsing \"0x1p1024\": value out of range"),
            Err("strconv.ParseFloat: parsing \"0x1p1024\": value out of range"),
        ),
        (
            b"\xff",
            Err("strconv.ParseFloat: parsing \"\\xff\": invalid syntax"),
            Err("strconv.ParseFloat: parsing \"\\xff\": invalid syntax"),
        ),
    ];
    for (s, f64_want, f32_want) in cases {
        for (bits, want) in [(64, f64_want), (32, f32_want)] {
            let got = text(go_strconv::parse_float(s, bits));
            match (got, want) {
                (Ok(g), Ok(w)) if w.is_nan() => assert!(g.is_nan(), "{s:?} {bits}"),
                (Ok(g), Ok(w)) => assert_eq!(g.to_bits(), w.to_bits(), "{s:?} {bits}"),
                (got, want) => assert_eq!(got, want.map_err(str::to_string), "{s:?} {bits}"),
            }
        }
    }
}

#[test]
fn test_format_float_shortest() {
    // f, then FormatFloat(f, 'e'|'E'|'f'|'g'|'G', -1, 64) and FormatFloat(f, 'g', -1, 32).
    let cases: [(f64, &str, &str, &str, &str, &str, &str); 17] = [
        (f64::from_bits(0x0), "0e+00", "0E+00", "0", "0", "0", "0"),
        (f64::from_bits(0x0), "0e+00", "0E+00", "0", "0", "0", "0"),
        (
            f64::from_bits(0x3ff0000000000000),
            "1e+00",
            "1E+00",
            "1",
            "1",
            "1",
            "1",
        ),
        (
            f64::from_bits(0xbff0000000000000),
            "-1e+00",
            "-1E+00",
            "-1",
            "-1",
            "-1",
            "-1",
        ),
        (
            f64::from_bits(0x3fb999999999999a),
            "1e-01",
            "1E-01",
            "0.1",
            "0.1",
            "0.1",
            "0.1",
        ),
        (
            f64::from_bits(0x444b1ae4d6e2ef50),
            "1e+21",
            "1E+21",
            "1000000000000000000000",
            "1e+21",
            "1E+21",
            "1e+21",
        ),
        (
            f64::from_bits(0x4415af1d78b58c40),
            "1e+20",
            "1E+20",
            "100000000000000000000",
            "1e+20",
            "1E+20",
            "1e+20",
        ),
        (
            f64::from_bits(0x419d6f3454000000),
            "1.23456789e+08",
            "1.23456789E+08",
            "123456789",
            "1.23456789e+08",
            "1.23456789E+08",
            "1.2345679e+08",
        ),
        (
            f64::from_bits(0x3e8421f5f40d8376),
            "1.5e-07",
            "1.5E-07",
            "0.00000015",
            "1.5e-07",
            "1.5E-07",
            "1.5e-07",
        ),
        (
            f64::from_bits(0x3f1a36e2eb1c432d),
            "1e-04",
            "1E-04",
            "0.0001",
            "0.0001",
            "0.0001",
            "0.0001",
        ),
        (
            f64::from_bits(0x3ee4f8b588e368f1),
            "1e-05",
            "1E-05",
            "0.00001",
            "1e-05",
            "1E-05",
            "1e-05",
        ),
        (
            f64::from_bits(0x1),
            "5e-324",
            "5E-324",
            "0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005",
            "5e-324",
            "5E-324",
            "0",
        ),
        (
            f64::from_bits(0x7fefffffffffffff),
            "1.7976931348623157e+308",
            "1.7976931348623157E+308",
            "179769313486231570000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
            "1.7976931348623157e+308",
            "1.7976931348623157E+308",
            "+Inf",
        ),
        (
            f64::from_bits(0x4004000000000000),
            "2.5e+00",
            "2.5E+00",
            "2.5",
            "2.5",
            "2.5",
            "2.5",
        ),
        (
            f64::from_bits(0x4059000000000000),
            "1e+02",
            "1E+02",
            "100",
            "100",
            "100",
            "100",
        ),
        (
            f64::from_bits(0x412e848000000000),
            "1e+06",
            "1E+06",
            "1000000",
            "1e+06",
            "1E+06",
            "1e+06",
        ),
        (
            f64::from_bits(0x40fe240c9fbe76c9),
            "1.23456789e+05",
            "1.23456789E+05",
            "123456.789",
            "123456.789",
            "123456.789",
            "123456.79",
        ),
    ];
    for (f, e, ue, fx, g, ug, g32) in cases {
        assert_eq!(go_strconv::format_float(f, b'e', 64), e);
        assert_eq!(go_strconv::format_float(f, b'E', 64), ue);
        assert_eq!(go_strconv::format_float(f, b'f', 64), fx);
        assert_eq!(go_strconv::format_float(f, b'g', 64), g);
        assert_eq!(go_strconv::format_float(f, b'G', 64), ug);
        assert_eq!(go_strconv::format_float(f, b'g', 32), g32);
    }
    assert_eq!(go_strconv::format_float(f64::NAN, b'g', 64), "NaN");
    assert_eq!(go_strconv::format_float(f64::INFINITY, b'e', 64), "+Inf");
    assert_eq!(
        go_strconv::format_float(f64::NEG_INFINITY, b'f', 32),
        "-Inf"
    );
    assert_eq!(go_strconv::format_float(-0.0, b'g', 64), "-0");
}

#[test]
fn test_quote() {
    let cases: [(&[u8], &str); 18] = [
        (b"", "\"\""),
        (b"abc", "\"abc\""),
        (b"a\x22b\x5cc", "\"a\\\"b\\\\c\""),
        (b"\x07\x08\x0c\x0a\x0d\x09\x0b", "\"\\a\\b\\f\\n\\r\\t\\v\""),
        (b"\x00\x1f\x7f", "\"\\x00\\x1f\\x7f\""),
        (b"\xc3\xa9", "\"é\""),
        (b"\xc2\xad", "\"\\u00ad\""),
        (b"\xe2\x80\x8b", "\"\\u200b\""),
        (b"\xef\xbb\xbf", "\"\\ufeff\""),
        (b"\xf0\x9f\x98\x80", "\"\u{1f600}\""),
        (b"\xf3\xa0\x80\x81", "\"\\U000e0001\""),
        (b"\xee\x80\x80", "\"\\ue000\""),
        (b"\xcd\xb8", "\"\\u0378\""),
        (b"\xff", "\"\\xff\""),
        (b"a\xe2\x82b", "\"a\\xe2\\x82b\""),
        (b"\xe2\x80\xa8", "\"\\u2028\""),
        (b"\xc2\xa0", "\"\\u00a0\""),
        (b" ", "\" \""),
    ];
    for (s, want) in cases {
        assert_eq!(go_strconv::quote(s), want, "{s:?}");
    }
    // A str quotes as its bytes do.
    assert_eq!(go_strconv::quote("a\tb"), "\"a\\tb\"");
    assert_eq!(go_strconv::quote(&String::from("\u{7f}")), "\"\\x7f\"");
}

#[test]
fn test_quote_rune() {
    let cases: [(char, &str); 10] = [
        ('\u{61}', "'a'"),
        ('\u{27}', "'\\''"),
        ('\u{22}', "'\"'"),
        ('\u{5c}', "'\\\\'"),
        ('\u{a}', "'\\n'"),
        ('\u{7f}', "'\\x7f'"),
        ('\u{ad}', "'\\u00ad'"),
        ('\u{1f600}', "'\u{1f600}'"),
        ('\u{e000}', "'\\ue000'"),
        ('\u{20}', "' '"),
    ];
    for (r, want) in cases {
        assert_eq!(go_strconv::quote_rune(r), want, "{r:?}");
    }
}

#[test]
fn test_unquote() {
    let cases: [(&str, Option<Vec<u8>>); 16] = [
        ("\"abc\"", Some(b"abc".to_vec())),
        ("\"a\\\"b\"", Some(b"a\x22b".to_vec())),
        (
            "\"\\x41é\\U0001F600\"",
            Some(b"A\xc3\xa9\xf0\x9f\x98\x80".to_vec()),
        ),
        ("\"\\101\"", Some(b"A".to_vec())),
        ("\"\\xff\"", Some(b"\xff".to_vec())),
        ("\"\\777\"", None),
        ("\"\\'\"", None),
        ("\"a", None),
        ("abc\"", None),
        ("\"a\"b\"", None),
        ("\"a\u{a}b\"", None),
        ("\"\\q\"", None),
        ("\"\\uD800\"", None),
        ("\"\"", Some(b"".to_vec())),
        ("\"", None),
        ("\"\\x4\"", None),
    ];
    for (s, want) in cases {
        assert_eq!(go_strconv::unquote(s), want, "{s:?}");
    }
}

#[test]
fn test_is_print_is_reexported() {
    assert!(go_strconv::is_print('a'));
    assert!(!go_strconv::is_print('\u{ad}'));
}

#[test]
fn test_parse_with_an_invalid_base_is_refused() {
    // Go's baseError; a base of 1 overflowed the port's cutoff.
    assert_eq!(
        text(go_strconv::parse_uint("10", 1)),
        Err(r#"strconv.ParseUint: parsing "10": invalid base 1"#.to_string())
    );
    assert_eq!(
        text(go_strconv::parse_uint("10", 37)),
        Err(r#"strconv.ParseUint: parsing "10": invalid base 37"#.to_string())
    );
    assert_eq!(
        text(go_strconv::parse_int("-10", 1)),
        Err(r#"strconv.ParseInt: parsing "-10": invalid base 1"#.to_string())
    );
    // An empty input is a syntax error before the base is checked, as in Go.
    assert_eq!(
        text(go_strconv::parse_uint("", 1)),
        Err(r#"strconv.ParseUint: parsing "": invalid syntax"#.to_string())
    );
    assert_eq!(go_strconv::parse_uint("z", 36), Ok(35));
}

#[test]
fn test_format_float_with_an_unknown_format_is_gos_text() {
    // Go's formatDigits appends '%' and the format byte; the port computed an exponent character from it.
    assert_eq!(go_strconv::format_float(1e21, 1, 64), "%\u{1}");
    assert_eq!(go_strconv::format_float(1.5, b'q', 64), "%q");
    assert_eq!(go_strconv::format_float(f64::NAN, b'q', 64), "NaN");
    assert_eq!(go_strconv::format_float(1e21, b'G', 64), "1E+21");
}
