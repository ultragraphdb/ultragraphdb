/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Dgraph's `types/facets` has no tests of its own; these are written from its code, with the facet expectations
//! of Dgraph's `chunker/rdf_parser_test.go` and `json_parser_test.go`.

use ultragraphdb_core_facets::{
    FacetValue, FacetsErrorEnum, copy_facets, facet_for, sort_and_validate, type_id_for, val_for,
};
use ultragraphdb_core_types::{TypeId, Value, parse_time};
use ultragraphdb_proto_dgraph::api;
use ultragraphdb_proto_dgraph::api::facet::ValType;
use ultragraphdb_proto_pb::pb;

fn facet(key: &str) -> api::Facet {
    api::Facet {
        key: key.to_string(),
        value: key.as_bytes().to_vec(),
        val_type: ValType::Int as i32,
        tokens: vec![format!("\u{1}{key}")],
        alias: "old".to_string(),
    }
}

fn keys(fs: &[api::Facet]) -> Vec<&str> {
    fs.iter().map(|f| f.key.as_str()).collect()
}

#[test]
fn sort_and_validate_accepts_an_empty_list() {
    assert!(sort_and_validate(&mut []).is_ok());
}

#[test]
fn sort_and_validate_sorts_by_key() {
    let mut fs = vec![facet("weight"), facet("close"), facet("since")];
    sort_and_validate(&mut fs).unwrap();
    assert_eq!(keys(&fs), ["close", "since", "weight"]);
}

#[test]
fn sort_and_validate_refuses_a_repeated_key() {
    let mut fs = vec![facet("weight"), facet("close"), facet("weight")];
    let err = sort_and_validate(&mut fs).unwrap_err();
    assert_eq!(err.0, FacetsErrorEnum::RepeatedKey("weight".to_string()));
    assert_eq!(
        err.to_string(),
        "Repeated keys are not allowed in facets. But got weight"
    );
}

fn params(all_keys: bool, keys: &[(&str, &str)]) -> pb::FacetParams {
    pb::FacetParams {
        all_keys,
        param: keys
            .iter()
            .map(|(k, a)| pb::FacetParam {
                key: k.to_string(),
                alias: a.to_string(),
            })
            .collect(),
    }
}

#[test]
fn copy_facets_without_params_copies_nothing() {
    assert!(copy_facets(&[facet("a")], None).is_empty());
    assert!(copy_facets(&[], Some(&params(true, &[]))).is_empty());
}

#[test]
fn copy_facets_with_all_keys_copies_every_facet_without_alias_or_tokens() {
    let fs = [facet("a"), facet("b")];
    let out = copy_facets(&fs, Some(&params(true, &[("b", "bee")])));
    assert_eq!(keys(&out), ["a", "b"]);
    for (copy, orig) in out.iter().zip(&fs) {
        assert_eq!(copy.value, orig.value);
        assert_eq!(copy.val_type, orig.val_type);
        assert!(copy.tokens.is_empty());
        assert_eq!(copy.alias, "");
    }
}

#[test]
fn copy_facets_copies_the_requested_keys_with_their_aliases() {
    let fs = [facet("a"), facet("c"), facet("d"), facet("f")];
    let out = copy_facets(
        &fs,
        Some(&params(false, &[("b", "bee"), ("c", "sea"), ("f", "")])),
    );
    assert_eq!(keys(&out), ["c", "f"]);
    assert_eq!(out[0].alias, "sea");
    assert_eq!(out[0].value, b"c");
    assert_eq!(out[1].alias, "");
}

#[test]
fn copy_facets_stops_at_the_last_requested_key() {
    let fs = [facet("a"), facet("b")];
    assert!(copy_facets(&fs, Some(&params(false, &[]))).is_empty());
    let out = copy_facets(&fs, Some(&params(false, &[("a", "x")])));
    assert_eq!(keys(&out), ["a"]);
}

fn expect(input: &str, val_type: ValType, value: &[u8], tokens: &[&str]) {
    let f = facet_for("key", input).unwrap_or_else(|e| panic!("{input}: {e}"));
    assert_eq!(f.key, "key");
    assert_eq!(f.val_type(), val_type, "{input}");
    assert_eq!(f.value, value, "{input}");
    assert_eq!(f.tokens, tokens, "{input}");
    assert_eq!(f.alias, "");
}

#[test]
fn facet_for_takes_rdf_parser_test_values() {
    // chunker/rdf_parser_test.go, the edge facet cases.
    expect(r#""hatter""#, ValType::String, b"hatter", &["\u{1}hatter"]);
    expect(
        r#""hatter \u0045""#,
        ValType::String,
        b"hatter E",
        &["\u{1}e", "\u{1}hatter"],
    );
    expect(r#""val1""#, ValType::String, b"val1", &["\u{1}val1"]);
    expect("", ValType::String, b"", &["\u{1}"]);
    expect("13", ValType::Int, b"\r\0\0\0\0\0\0\0", &[]);
    expect("12", ValType::Int, b"\x0c\0\0\0\0\0\0\0", &[]);
    expect("0x0D", ValType::Int, b"\r\0\0\0\0\0\0\0", &[]);
    expect("1.2", ValType::Float, b"333333\xf3?", &[]);
    expect(
        "111111111111111111888888.23",
        ValType::Float,
        b"\xa0\xa8OlX\x87\xb7D",
        &[],
    );
    expect("true", ValType::Bool, b"\x01", &[]);
    expect("false", ValType::Bool, b"\x00", &[]);
    expect(
        "2006-01-02T15:04:05",
        ValType::Datetime,
        b"\x01\0\0\0\x0e\xbbK7\xe5\0\0\0\0\xff\xff",
        &[],
    );
    expect(
        "2002-10-02T15:00:00.05Z",
        ValType::Datetime,
        b"\x01\0\0\0\x0e\xb5-\0\xf0\x02\xfa\xf0\x80\xff\xff",
        &[],
    );
    expect(
        "2006-01-02T00:00:00Z",
        ValType::Datetime,
        b"\x01\0\0\0\x0e\xbbJd\0\0\0\0\0\xff\xff",
        &[],
    );
    expect(
        r#""\"hello world\"""#,
        ValType::String,
        br#""hello world""#,
        &["\u{1}hello", "\u{1}world"],
    );
    expect(
        r#""LineA\nLineB""#,
        ValType::String,
        b"LineA\nLineB",
        &["\u{1}linea", "\u{1}lineb"],
    );
}

#[test]
fn facet_for_refuses_what_rdf_parser_test_refuses() {
    for input in ["111111111111111111888888", "0x1787586C4FA8A0284FF8", "NaN"] {
        assert!(facet_for("k", input).is_err(), "{input}");
    }
}

#[test]
fn facet_for_tells_why() {
    let msg = |v: &str| facet_for("k", v).unwrap_err().to_string();
    assert_eq!(msg("abc"), "Could not parse the facet value : [abc]");
    assert_eq!(msg("NaN"), "Got invalid value: NaN");
    assert_eq!(
        msg("111111111111111111888888"),
        r#"strconv.ParseInt: parsing "111111111111111111888888": value out of range"#
    );
    assert_eq!(
        msg("1e400"),
        r#"strconv.ParseFloat: parsing "1e400": value out of range"#
    );
    assert_eq!(
        msg(r#""\'""#),
        r#"could not unquote "\"\\'\"":: invalid syntax"#
    );
}

#[test]
fn facet_for_keeps_bytes_a_hex_escape_writes() {
    let f = facet_for("k", r#""a\xffb c""#).unwrap();
    assert_eq!(f.value, b"a\xffb c");
    // The term tokenizer splits at the byte that is not UTF-8, as Go's does.
    assert_eq!(f.tokens, ["\u{1}a", "\u{1}b", "\u{1}c"]);
    assert_eq!(facet_for("k", r#""\377""#).unwrap().value, [0xff]);
}

#[test]
fn facet_for_does_not_trim_the_value() {
    assert!(facet_for("k", " 13").is_err());
    assert!(facet_for("k", "13 ").is_err());
}

#[test]
fn to_binary_encodes_the_value_of_its_type() {
    // Go's ToBinary takes a value and a type, and its unchecked type assertion panics when they disagree (ledger
    // 20); a FacetValue carries its type.
    let t = parse_time("2006-01-02T15:04:05Z").unwrap();
    let cases: [(FacetValue, ValType, Vec<u8>); 5] = [
        (
            FacetValue::Str(b"READ WRITE".to_vec()),
            ValType::String,
            b"READ WRITE".to_vec(),
        ),
        (
            FacetValue::Int(3),
            ValType::Int,
            3i64.to_le_bytes().to_vec(),
        ),
        (
            FacetValue::Float(30000.56),
            ValType::Float,
            30000.56f64.to_bits().to_le_bytes().to_vec(),
        ),
        (FacetValue::Bool(true), ValType::Bool, vec![1]),
        (
            FacetValue::DateTime(t),
            ValType::Datetime,
            t.marshal_binary().unwrap(),
        ),
    ];
    for (v, vt, bytes) in cases {
        let f = v.to_binary("car").unwrap();
        assert_eq!(f.key, "car");
        assert_eq!(f.val_type(), vt);
        assert_eq!(f.value, bytes);
        assert!(
            f.tokens.is_empty(),
            "to_binary leaves the tokens to facet_for"
        );
    }
}

#[test]
fn a_facet_value_encodes_its_variant() {
    let t = parse_time("2006-01-02T15:04:05Z").unwrap();
    let cases = [
        (
            FacetValue::Str(b"\xff".to_vec()),
            ValType::String,
            b"\xff".to_vec(),
        ),
        (
            FacetValue::Int(3),
            ValType::Int,
            3i64.to_le_bytes().to_vec(),
        ),
        (
            FacetValue::Float(0.5),
            ValType::Float,
            0.5f64.to_bits().to_le_bytes().to_vec(),
        ),
        (FacetValue::Bool(true), ValType::Bool, vec![1]),
        (
            FacetValue::DateTime(t),
            ValType::Datetime,
            t.marshal_binary().unwrap(),
        ),
    ];
    for (v, vt, bytes) in cases {
        assert_eq!(v.val_type(), vt);
        let f = v.to_binary("car").unwrap();
        assert_eq!((f.key.as_str(), f.val_type(), f.value), ("car", vt, bytes));
        assert!(f.tokens.is_empty() && f.alias.is_empty());
    }
}

#[test]
fn type_id_for_maps_every_facet_type() {
    let tid = |vt: i32| {
        type_id_for(&api::Facet {
            val_type: vt,
            ..Default::default()
        })
    };
    assert_eq!(tid(ValType::Int as i32).unwrap(), TypeId::INT);
    assert_eq!(tid(ValType::Float as i32).unwrap(), TypeId::FLOAT);
    assert_eq!(tid(ValType::Bool as i32).unwrap(), TypeId::BOOL);
    assert_eq!(tid(ValType::Datetime as i32).unwrap(), TypeId::DATETIME);
    assert_eq!(tid(ValType::String as i32).unwrap(), TypeId::STRING);
    let err = tid(9).unwrap_err();
    assert_eq!(err.0, FacetsErrorEnum::UnrecognizedType(9));
    assert_eq!(err.to_string(), "Unrecognized facet type: 9");
}

#[test]
fn val_for_decodes_what_facet_for_encodes() {
    let v = |s: &str| val_for(&facet_for("k", s).unwrap()).unwrap();
    assert_eq!(v("0x10").value(), Some(&Value::Int(16)));
    assert_eq!(v("0.7").value(), Some(&Value::Float(0.7)));
    assert_eq!(v("true").value(), Some(&Value::Bool(true)));
    assert_eq!(v(r#""x""#).value(), Some(&Value::str("x")));
    assert_eq!(v(r#""x""#).tid(), TypeId::STRING);
    assert_eq!(
        v("2006-01-02T15:04:05").value(),
        Some(&Value::DateTime(parse_time("2006-01-02T15:04:05").unwrap()))
    );
}

#[test]
fn val_for_refuses_an_unknown_type() {
    let f = api::Facet {
        val_type: 5,
        ..Default::default()
    };
    assert!(val_for(&f).is_err());
}
