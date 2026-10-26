/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! `token_value` over the tokenizer-datatype table of the index-maintenance specification: each tokenizer's values
//! from the datatypes it takes, equal values of different lexical forms alike, and `None` for every datatype it does
//! not take and every lexical form not of its datatype. Expected values are written out by hand; a token's bytes
//! follow Dgraph's `encodeInt` (a sign byte, 1 for non-negative, then the value big-endian) and its date tokens (each
//! UTC field a big-endian u16).

use oxrdf::vocab::xsd;
use oxrdf::{BaseDirection, Literal, NamedNode};
use ultragraphdb_core_tok::get_tokenizer;
use ultragraphdb_core_types::Value;
use ultragraphdb_store_apply::token_value;

const GEO_JSON: &str = "http://www.opengis.net/ont/geosparql#geoJSONLiteral";

fn typed(lexical: &str, datatype: oxrdf::NamedNodeRef<'_>) -> Literal {
    Literal::new_typed_literal(lexical, datatype)
}

fn value(literal: &Literal, tokenizer: &str) -> Option<Value> {
    token_value(literal.as_ref(), tokenizer)
}

/// The tokens `tokenizer` makes of the value `token_value` gives `literal`.
fn tokens(literal: &Literal, tokenizer: &str) -> Vec<Vec<u8>> {
    let v =
        value(literal, tokenizer).unwrap_or_else(|| panic!("{literal} has a {tokenizer} value"));
    get_tokenizer(tokenizer)
        .expect("a tokenizer")
        .tokens(&v)
        .expect("the value tokenizes")
}

#[test]
fn test_int_takes_the_integers_by_value_within_64_bits() {
    let cases = [
        (typed("042", xsd::INTEGER), Some(Value::Int(42))),
        (typed("+42", xsd::INTEGER), Some(Value::Int(42))),
        (typed("-7", xsd::LONG), Some(Value::Int(-7))),
        (typed("5", xsd::BYTE), Some(Value::Int(5))),
        (typed("0", xsd::NON_NEGATIVE_INTEGER), Some(Value::Int(0))),
        (typed("-1", xsd::NEGATIVE_INTEGER), Some(Value::Int(-1))),
        // i64::MAX, 2^63 - 1: the largest value within 64 bits.
        (
            typed("9223372036854775807", xsd::INTEGER),
            Some(Value::Int(i64::MAX)),
        ),
        // i64::MIN, -2^63.
        (
            typed("-9223372036854775808", xsd::INTEGER),
            Some(Value::Int(i64::MIN)),
        ),
        // 2^63, one past i64::MAX, and u64::MAX as an unsignedLong: beyond 64 signed bits, held unindexed.
        (typed("9223372036854775808", xsd::INTEGER), None),
        (typed("18446744073709551615", xsd::UNSIGNED_LONG), None),
        // A lexical form not of its datatype, and datatypes `int` does not take.
        (typed("4.2", xsd::INTEGER), None),
        (Literal::new_simple_literal("42"), None),
        (typed("42", xsd::DECIMAL), None),
        (typed("42", xsd::DOUBLE), None),
    ];
    for (literal, expected) in cases {
        assert_eq!(value(&literal, "int"), expected, "{literal}");
    }
    // encodeInt(42) and encodeInt(-1).
    assert_eq!(
        tokens(&typed("042", xsd::INTEGER), "int"),
        vec![vec![1, 0, 0, 0, 0, 0, 0, 0, 42]]
    );
    assert_eq!(
        tokens(&typed("-1", xsd::INTEGER), "int"),
        vec![vec![0, 255, 255, 255, 255, 255, 255, 255, 255]]
    );
}

#[test]
fn test_float_takes_doubles_and_floats_each_at_its_precision() {
    // 42.5 is exact in binary; 4.25E1 is a non-canonical form of it.
    assert_eq!(
        value(&typed("4.25E1", xsd::DOUBLE), "float"),
        Some(Value::Float(42.5))
    );
    assert_eq!(
        value(&typed("42.5", xsd::FLOAT), "float"),
        Some(Value::Float(42.5))
    );
    // 0.1 as a 32-bit float widened is 0.100000001490116119384765625, not the double 0.1.
    assert_eq!(
        value(&typed("0.1", xsd::FLOAT), "float"),
        Some(Value::Float(0.100_000_001_490_116_12))
    );
    assert_eq!(
        value(&typed("0.1", xsd::DOUBLE), "float"),
        Some(Value::Float(0.1))
    );
    assert_eq!(
        value(&typed("INF", xsd::DOUBLE), "float"),
        Some(Value::Float(f64::INFINITY))
    );
    assert!(
        matches!(value(&typed("NaN", xsd::DOUBLE), "float"), Some(Value::Float(f)) if f.is_nan())
    );
    assert_eq!(value(&typed("abc", xsd::DOUBLE), "float"), None);
    assert_eq!(value(&typed("1.5", xsd::DECIMAL), "float"), None);
    assert_eq!(value(&typed("1", xsd::INTEGER), "float"), None);
    // The float token truncates to an integer: encodeInt(42).
    assert_eq!(
        tokens(&typed("42.5", xsd::DOUBLE), "float"),
        vec![vec![1, 0, 0, 0, 0, 0, 0, 0, 42]]
    );
}

#[test]
fn test_bool_takes_both_lexical_forms_of_each_value() {
    assert_eq!(
        value(&typed("true", xsd::BOOLEAN), "bool"),
        Some(Value::Bool(true))
    );
    assert_eq!(
        value(&typed("1", xsd::BOOLEAN), "bool"),
        Some(Value::Bool(true))
    );
    assert_eq!(
        value(&typed("false", xsd::BOOLEAN), "bool"),
        Some(Value::Bool(false))
    );
    assert_eq!(
        value(&typed("0", xsd::BOOLEAN), "bool"),
        Some(Value::Bool(false))
    );
    assert_eq!(value(&typed("yes", xsd::BOOLEAN), "bool"), None);
    assert_eq!(value(&Literal::new_simple_literal("true"), "bool"), None);
    // encodeInt(1) and encodeInt(0).
    assert_eq!(
        tokens(&typed("1", xsd::BOOLEAN), "bool"),
        vec![vec![1, 0, 0, 0, 0, 0, 0, 0, 1]]
    );
    assert_eq!(
        tokens(&typed("false", xsd::BOOLEAN), "bool"),
        vec![vec![1, 0, 0, 0, 0, 0, 0, 0, 0]]
    );
}

#[test]
fn test_bigfloat_takes_decimals() {
    // 2.50 and 2.5 are one value; the bigfloat token is encodeInt of its integer part, 2.
    let two = vec![vec![1, 0, 0, 0, 0, 0, 0, 0, 2]];
    assert_eq!(tokens(&typed("2.50", xsd::DECIMAL), "bigfloat"), two);
    assert_eq!(tokens(&typed("2.5", xsd::DECIMAL), "bigfloat"), two);
    // 123456789: 0x075BCD15.
    assert_eq!(
        tokens(&typed("123456789", xsd::DECIMAL), "bigfloat"),
        vec![vec![1, 0, 0, 0, 0, 0x07, 0x5B, 0xCD, 0x15]]
    );
    assert_eq!(value(&typed("2,5", xsd::DECIMAL), "bigfloat"), None);
    assert_eq!(value(&typed("2.5", xsd::DOUBLE), "bigfloat"), None);
}

#[test]
fn test_the_date_tokenizers_take_date_times_as_utc_and_dates_but_for_hour() {
    // 2020 = 0x07E4, May = 5, the 17th = 0x11, 13 h = 0x0D.
    let at = typed("2020-05-17T13:45:00Z", xsd::DATE_TIME);
    assert_eq!(tokens(&at, "year"), vec![vec![0x07, 0xE4]]);
    assert_eq!(tokens(&at, "month"), vec![vec![0x07, 0xE4, 0, 5]]);
    assert_eq!(tokens(&at, "day"), vec![vec![0x07, 0xE4, 0, 5, 0, 0x11]]);
    assert_eq!(
        tokens(&at, "hour"),
        vec![vec![0x07, 0xE4, 0, 5, 0, 0x11, 0, 0x0D]]
    );
    // No time zone: taken as UTC, so 13 h stays 13 h.
    let local = typed("2020-05-17T13:45:00", xsd::DATE_TIME);
    assert_eq!(
        tokens(&local, "hour"),
        vec![vec![0x07, 0xE4, 0, 5, 0, 0x11, 0, 0x0D]]
    );
    // 23:30 at -02:00 is 01:30 UTC the next day, in the next year: 2021 = 0x07E5.
    let offset = typed("2020-12-31T23:30:00-02:00", xsd::DATE_TIME);
    assert_eq!(
        tokens(&offset, "hour"),
        vec![vec![0x07, 0xE5, 0, 1, 0, 1, 0, 1]]
    );
    let stamp = typed("2020-05-17T13:45:00Z", xsd::DATE_TIME_STAMP);
    assert_eq!(tokens(&stamp, "year"), vec![vec![0x07, 0xE4]]);
    let date = typed("2020-05-17", xsd::DATE);
    assert_eq!(tokens(&date, "day"), vec![vec![0x07, 0xE4, 0, 5, 0, 0x11]]);
    assert_eq!(value(&date, "hour"), None);
    assert_eq!(
        value(&typed("2020-13-01T00:00:00Z", xsd::DATE_TIME), "year"),
        None
    );
    assert_eq!(value(&typed("2020-05-17", xsd::STRING), "year"), None);
}

#[test]
fn test_the_string_tokenizers_take_strings_and_language_strings_as_written() {
    let plain = Literal::new_simple_literal("Big Apple");
    let tagged = Literal::new_language_tagged_literal_unchecked("Big Apple", "en");
    let directional = Literal::new_directional_language_tagged_literal_unchecked(
        "Big Apple",
        "en",
        BaseDirection::Ltr,
    );
    for tokenizer in [
        "exact", "hash", "term", "fulltext", "trigram", "ngram", "sha256",
    ] {
        for literal in [&plain, &tagged, &directional] {
            assert_eq!(
                value(literal, tokenizer),
                Some(Value::Str("Big Apple".into())),
                "{tokenizer} {literal}"
            );
        }
        assert_eq!(
            value(&typed("7", xsd::INTEGER), tokenizer),
            None,
            "{tokenizer}"
        );
    }
    // The empty string is a string.
    assert_eq!(
        value(&Literal::new_simple_literal(""), "exact"),
        Some(Value::Str(String::new()))
    );
    // The term tokenizer lowercases and splits at the space.
    assert_eq!(
        tokens(&plain, "term"),
        vec![b"apple".to_vec(), b"big".to_vec()]
    );
    // The exact token is the string's bytes.
    assert_eq!(tokens(&plain, "exact"), vec![b"Big Apple".to_vec()]);
    // BLAKE2b-256 of "abc", RFC 7693's test string: bddd813c…68d52319.
    let abc = tokens(&Literal::new_simple_literal("abc"), "hash");
    assert_eq!(
        abc,
        vec![vec![
            0xbd, 0xdd, 0x81, 0x3c, 0x63, 0x42, 0x39, 0x72, 0x31, 0x71, 0xef, 0x3f, 0xee, 0x98,
            0x57, 0x9b, 0x94, 0x96, 0x4e, 0x3b, 0xb1, 0xcb, 0x3e, 0x42, 0x72, 0x62, 0xc8, 0xc0,
            0x68, 0xd5, 0x23, 0x19,
        ]]
    );
}

#[test]
fn test_geo_takes_geojson_geometries() {
    let geo = NamedNode::new_unchecked(GEO_JSON);
    let point = typed(
        r#"{"type": "Point", "coordinates": [8.63, 50.12]}"#,
        geo.as_ref(),
    );
    assert!(matches!(value(&point, "geo"), Some(Value::Geo(_))));
    assert!(!tokens(&point, "geo").is_empty());
    assert_eq!(value(&typed("{}", geo.as_ref()), "geo"), None);
    assert_eq!(value(&typed("not json", geo.as_ref()), "geo"), None);
    assert_eq!(value(&point, "exact"), None);
    assert_eq!(
        value(
            &Literal::new_simple_literal(r#"{"type": "Point", "coordinates": [8.63, 50.12]}"#),
            "geo"
        ),
        None
    );
}

#[test]
fn test_an_unknown_tokenizer_takes_nothing() {
    assert_eq!(value(&Literal::new_simple_literal("x"), "nope"), None);
    assert_eq!(value(&typed("1", xsd::INTEGER), ""), None);
}

#[test]
fn test_a_date_with_a_time_zone_is_indexed_at_midnight_in_its_zone_taken_to_utc() {
    // Each date is read as midnight in its own zone, then taken to UTC, as XSD casts a date to a date-time. Hand-derived:
    // midnight at +hh:mm is hh:mm before midnight UTC, the previous day; at -hh:mm it is hh:mm after, the same day.
    // Year 2020 = 0x07E4, 2021 = 0x07E5.
    let cases: [(&str, [u8; 6]); 9] = [
        // Z: midnight UTC, 2020-05-17.
        ("2020-05-17Z", [0x07, 0xE4, 0, 5, 0, 17]),
        // +02:00: 2020-05-16T22:00Z, the day before.
        ("2020-05-17+02:00", [0x07, 0xE4, 0, 5, 0, 16]),
        // -05:00: 2020-05-17T05:00Z, the same day.
        ("2020-05-17-05:00", [0x07, 0xE4, 0, 5, 0, 17]),
        // +14:00, the easternmost zone: 2020-05-16T10:00Z.
        ("2020-05-17+14:00", [0x07, 0xE4, 0, 5, 0, 16]),
        // -14:00, the westernmost: 2020-05-17T14:00Z.
        ("2020-05-17-14:00", [0x07, 0xE4, 0, 5, 0, 17]),
        // Across a month, in a leap year: 2020-03-01+01:00 is 2020-02-29T23:00Z.
        ("2020-03-01+01:00", [0x07, 0xE4, 0, 2, 0, 29]),
        // Across a year: 2021-01-01+02:00 is 2020-12-31T22:00Z.
        ("2021-01-01+02:00", [0x07, 0xE4, 0, 12, 0, 31]),
        // The same date at -14:00 stays in 2021: 2021-01-01T14:00Z.
        ("2021-01-01-14:00", [0x07, 0xE5, 0, 1, 0, 1]),
        // Without a zone the date is taken as UTC, unchanged.
        ("2021-01-01", [0x07, 0xE5, 0, 1, 0, 1]),
    ];
    for (lexical, day) in cases {
        let date = typed(lexical, xsd::DATE);
        assert_eq!(tokens(&date, "year"), vec![day[..2].to_vec()], "{lexical}");
        assert_eq!(tokens(&date, "month"), vec![day[..4].to_vec()], "{lexical}");
        assert_eq!(tokens(&date, "day"), vec![day.to_vec()], "{lexical}");
        assert_eq!(value(&date, "hour"), None, "{lexical}");
    }
}
