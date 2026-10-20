/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_tok::{
    OptionValue, float32_opt_parser, float64_opt_parser, int_opt_parser, string_opt_parser,
    uint_opt_parser,
};

#[test]
fn test_int_opt_parser() {
    assert_eq!(
        int_opt_parser("-9223372036854775808").unwrap(),
        OptionValue::Int(i64::MIN)
    );
    assert_eq!(
        int_opt_parser("9223372036854775808")
            .unwrap_err()
            .to_string(),
        "strconv.Atoi: parsing \"9223372036854775808\": value out of range"
    );
    assert_eq!(int_opt_parser("+7").unwrap(), OptionValue::Int(7));
    assert_eq!(
        int_opt_parser("-").unwrap_err().to_string(),
        "strconv.Atoi: parsing \"-\": invalid syntax"
    );
}

#[test]
fn test_uint_opt_parser() {
    assert_eq!(
        uint_opt_parser("18446744073709551615").unwrap(),
        OptionValue::Uint(u64::MAX)
    );
    assert_eq!(
        uint_opt_parser("-1").unwrap_err().to_string(),
        "strconv.ParseUint: parsing \"-1\": invalid syntax"
    );
    assert_eq!(
        uint_opt_parser("18446744073709551616")
            .unwrap_err()
            .to_string(),
        "strconv.ParseUint: parsing \"18446744073709551616\": value out of range"
    );
}

#[test]
fn test_float_and_string_opt_parsers() {
    assert_eq!(
        string_opt_parser("a b").unwrap(),
        OptionValue::Str("a b".into())
    );
    assert_eq!(
        float64_opt_parser("-2.5e3").unwrap(),
        OptionValue::Float(-2500.0)
    );
    assert_eq!(
        float64_opt_parser("x").unwrap_err().to_string(),
        "strconv.ParseFloat: parsing \"x\": invalid syntax"
    );
    assert_eq!(
        float64_opt_parser("1e999").unwrap_err().to_string(),
        "strconv.ParseFloat: parsing \"1e999\": value out of range"
    );
    assert!(matches!(float64_opt_parser("Inf").unwrap(), OptionValue::Float(f) if f.is_infinite()));
    assert_eq!(
        float32_opt_parser("0.1").unwrap(),
        OptionValue::Float(f64::from(0.1f32))
    );
    assert_eq!(
        float32_opt_parser("1e39").unwrap_err().to_string(),
        "strconv.ParseFloat: parsing \"1e39\": value out of range"
    );
}
