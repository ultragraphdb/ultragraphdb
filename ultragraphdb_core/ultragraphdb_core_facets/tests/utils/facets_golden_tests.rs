/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Golden vectors from `tools/golden/facets.go`.

use serde::Deserialize;
use ultragraphdb_core_facets::{facet_for, sort_and_validate, to_binary, val_for};
use ultragraphdb_core_types::{DateTime, TypeId, Value, marshal, parse_time};
use ultragraphdb_proto_dgraph::api;
use ultragraphdb_proto_dgraph::api::facet::ValType;
use ultragraphdb_test_bdd::{Golden, compare_bytes, load_golden, testdata_dir};

fn golden<T: for<'de> Deserialize<'de>>(name: &str) -> Golden<T> {
    load_golden(&testdata_dir().join(format!("golden/{name}.json")))
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect()
}

#[derive(Debug, Deserialize)]
struct FacetJson {
    key: String,
    value: String,
    val_type: i32,
    tokens: Vec<String>,
}

fn compare_facet(input: &str, want: &FacetJson, got: &api::Facet) -> Result<(), String> {
    if got.key != want.key || got.val_type != want.val_type {
        return Err(format!(
            "{input:?}: key/type {:?}/{} != {:?}/{}",
            got.key, got.val_type, want.key, want.val_type
        ));
    }
    compare_bytes(input, &unhex(&want.value), &got.value)?;
    let want_tokens: Vec<Vec<u8>> = want.tokens.iter().map(|t| unhex(t)).collect();
    let got_tokens: Vec<Vec<u8>> = got.tokens.iter().map(|t| t.as_bytes().to_vec()).collect();
    if want_tokens != got_tokens {
        return Err(format!(
            "{input:?}: tokens {got_tokens:?} != {want_tokens:?}"
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct ValForJson {
    tid: i32,
    str: String,
    err: String,
}

#[derive(Debug, Deserialize)]
struct FacetForCase {
    key: String,
    val: String,
    err: String,
    facet: Option<FacetJson>,
    val_for: Option<ValForJson>,
}

fn check_facet_for(c: &FacetForCase) -> Result<(), String> {
    let got = facet_for(&c.key, &c.val);
    let (want, f) = match (&c.facet, got) {
        (None, Err(e)) if e.to_string() == c.err => return Ok(()),
        (Some(want), Ok(f)) => (want, f),
        (_, got) => {
            return Err(format!(
                "{:?}: {got:?}, want {:?} / {:?}",
                c.val, c.facet, c.err
            ));
        }
    };
    compare_facet(&c.val, want, &f)?;
    let want_vf = c.val_for.as_ref().expect("val_for");
    assert!(want_vf.err.is_empty(), "the generator decodes every facet");
    let v = val_for(&f).map_err(|e| format!("{:?}: val_for: {e}", c.val))?;
    let s = marshal(&v, TypeId::STRING).map_err(|e| format!("{:?}: marshal: {e}", c.val))?;
    let s = match s.value() {
        Some(Value::Str(s)) => s.clone(),
        other => return Err(format!("{:?}: {other:?}", c.val)),
    };
    // Go's JSON encoder writes each byte that is not UTF-8 as U+FFFD.
    if v.tid().enum_value() != want_vf.tid || String::from_utf8_lossy(&s) != want_vf.str {
        return Err(format!(
            "{:?}: val_for {:?} {:?} != {} {:?}",
            c.val,
            v.tid(),
            String::from_utf8_lossy(&s),
            want_vf.tid,
            want_vf.str
        ));
    }
    Ok(())
}

#[test]
fn facet_for_matches_go() {
    let g = golden::<FacetForCase>("facets_facet_for");
    assert!(g.cases().len() > 100);
    let failures: Vec<String> = g
        .cases()
        .iter()
        .filter_map(|c| check_facet_for(c).err())
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[derive(Debug, Deserialize)]
struct ToBinaryCase {
    name: String,
    val_type: i32,
    err: String,
    panic: String,
    facet: Option<FacetJson>,
}

fn to_binary_input(name: &str) -> Value {
    match name {
        "string" => Value::str("READ WRITE"),
        "empty string" => Value::str(""),
        "int" | "int as string" | "int as bool" | "unknown type" => {
            Value::Int(if name == "int" { 3 } else { 1 })
        }
        "negative int" => Value::Int(-1),
        "float" => Value::Float(30000.56),
        "inf" => Value::Float(f64::INFINITY),
        "true" | "bool as float" => Value::Bool(true),
        "false" => Value::Bool(false),
        "datetime" => Value::DateTime(parse_time("2006-01-02T15:04:05+07:00").unwrap()),
        "utc datetime" => Value::DateTime(DateTime::utc(2006, 1, 2, 15, 4, 5, 0)),
        "string as int" => Value::str("3"),
        "float as int" => Value::Float(1.5),
        "string as datetime" => Value::str("2006"),
        other => panic!("no input for {other}"),
    }
}

#[test]
fn to_binary_matches_go() {
    let g = golden::<ToBinaryCase>("facets_to_binary");
    for c in g.cases() {
        let vt = ValType::try_from(c.val_type).unwrap_or(ValType::String);
        let raw = api::Facet {
            val_type: c.val_type,
            ..Default::default()
        };
        let got = if raw.val_type != vt as i32 {
            // A type the enum does not name: to_binary takes the enum, so ask type_id_for, which refuses it as Go
            // does.
            ultragraphdb_core_facets::type_id_for(&raw).map(|_| unreachable!())
        } else {
            to_binary("k", &to_binary_input(&c.name), vt)
        };
        match (&c.facet, got) {
            (Some(want), Ok(f)) => compare_facet(&c.name, want, &f).unwrap(),
            // Go panics on its unchecked type assertion; the port refuses.
            (None, Err(_)) if !c.panic.is_empty() => {}
            (None, Err(e)) => assert_eq!(e.to_string(), c.err, "{}", c.name),
            (_, got) => panic!("{}: {got:?}, want {:?} / {:?}", c.name, c.facet, c.err),
        }
    }
}

#[derive(Debug, Deserialize)]
struct SortCase {
    keys: Vec<String>,
    sorted: Vec<String>,
    err: String,
}

#[test]
fn sort_and_validate_matches_go() {
    let g = golden::<SortCase>("facets_sort_and_validate");
    for c in g.cases() {
        let mut fs: Vec<api::Facet> = c
            .keys
            .iter()
            .map(|k| api::Facet {
                key: k.clone(),
                ..Default::default()
            })
            .collect();
        match sort_and_validate(&mut fs) {
            Ok(()) => {
                assert!(c.err.is_empty(), "{:?}", c.keys);
                let keys: Vec<&String> = fs.iter().map(|f| &f.key).collect();
                assert_eq!(keys, c.sorted.iter().collect::<Vec<_>>());
            }
            Err(e) => assert_eq!(e.to_string(), c.err, "{:?}", c.keys),
        }
    }
}
