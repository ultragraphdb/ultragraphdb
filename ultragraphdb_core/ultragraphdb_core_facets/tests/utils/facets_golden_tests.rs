/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Golden vectors from `tools/golden/facets.go`.

use serde::Deserialize;
use ultragraphdb_core_facets::{FacetValue, facet_for, sort_and_validate, val_for};
use ultragraphdb_core_types::{DateTime, TypeId, Value, marshal, parse_time};
use ultragraphdb_proto_dgraph::api;
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

/// The golden cases the port cannot express, with the ledger entry (`openspec/notes/dgraph-bugs-found-in-port.md`)
/// and what the port has instead. Each hands Go's `ToBinary` a value of another Go type than the facet type names,
/// on which `Marshal`'s unchecked assertion panics (ledger 20); a [`FacetValue`] carries its type, so no such pair
/// exists.
const DEVIATIONS: &[(&str, u32, &str)] = &[
    (
        "int as string",
        20,
        "not expressible: FacetValue carries its type",
    ),
    (
        "string as int",
        20,
        "not expressible: FacetValue carries its type",
    ),
    (
        "float as int",
        20,
        "not expressible: FacetValue carries its type",
    ),
    (
        "bool as float",
        20,
        "not expressible: FacetValue carries its type",
    ),
    (
        "int as bool",
        20,
        "not expressible: FacetValue carries its type",
    ),
    (
        "string as datetime",
        20,
        "not expressible: FacetValue carries its type",
    ),
];

/// The facet value of a golden case Go encodes, `None` for the deviations and the unknown type.
fn to_binary_input(name: &str) -> Option<FacetValue> {
    Some(match name {
        "string" => FacetValue::Str(b"READ WRITE".to_vec()),
        "empty string" => FacetValue::Str(Vec::new()),
        "int" => FacetValue::Int(3),
        "negative int" => FacetValue::Int(-1),
        "float" => FacetValue::Float(30000.56),
        "inf" => FacetValue::Float(f64::INFINITY),
        "true" => FacetValue::Bool(true),
        "false" => FacetValue::Bool(false),
        "datetime" => FacetValue::DateTime(parse_time("2006-01-02T15:04:05+07:00").unwrap()),
        "utc datetime" => FacetValue::DateTime(DateTime::utc(2006, 1, 2, 15, 4, 5, 0)),
        _ => return None,
    })
}

#[test]
fn to_binary_matches_go() {
    let g = golden::<ToBinaryCase>("facets_to_binary");
    let mut deviating = 0;
    for c in g.cases() {
        let deviation = DEVIATIONS.iter().find(|(name, _, _)| *name == c.name);
        assert_eq!(
            !c.panic.is_empty(),
            deviation.is_some(),
            "{}: Go panics exactly on the deviating cases",
            c.name
        );
        if deviation.is_some() {
            assert!(to_binary_input(&c.name).is_none(), "{}", c.name);
            deviating += 1;
            continue;
        }
        let got = match to_binary_input(&c.name) {
            Some(value) => {
                assert_eq!(value.val_type() as i32, c.val_type, "{}", c.name);
                value.to_binary("k")
            }
            // A type the enum does not name: a facet value carries one it names, so ask type_id_for, which refuses
            // it as Go does.
            None => ultragraphdb_core_facets::type_id_for(&api::Facet {
                val_type: c.val_type,
                ..Default::default()
            })
            .map(|_| api::Facet::default()),
        };
        match (&c.facet, got) {
            (Some(want), Ok(f)) => compare_facet(&c.name, want, &f).unwrap(),
            (None, Err(e)) => assert_eq!(e.to_string(), c.err, "{}", c.name),
            (_, got) => panic!("{}: {got:?}, want {:?} / {:?}", c.name, c.facet, c.err),
        }
    }
    assert_eq!(deviating, DEVIATIONS.len());
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
