/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use cucumber::{World, then, when};
use ultragraphdb_core_facets::{FacetsError, facet_for, sort_and_validate, val_for};
use ultragraphdb_core_types::Value;
use ultragraphdb_proto_dgraph::api;
use ultragraphdb_proto_dgraph::api::facet::ValType;

#[derive(Debug, Default, World)]
struct Steps {
    facets: Vec<api::Facet>,
    error: Option<FacetsError>,
}

/// The `key=value` pairs written in backticks.
fn pairs(text: &str) -> Vec<(String, String)> {
    text.split('`')
        .skip(1)
        .step_by(2)
        .map(|kv| {
            let (k, v) = kv.split_once('=').expect("key=value");
            (k.to_string(), v.to_string())
        })
        .collect()
}

fn parse(s: &mut Steps, text: &str) {
    for (k, v) in pairs(text) {
        match facet_for(&k, &v) {
            Ok(f) => s.facets.push(f),
            Err(e) => s.error = Some(e),
        }
    }
}

#[when(regex = r"^the facets (.+) are parsed$")]
fn the_facets_are_parsed(s: &mut Steps, text: String) {
    parse(s, &text);
}

#[when(regex = r"^the facet (`[^`]+`) is parsed$")]
fn the_facet_is_parsed(s: &mut Steps, text: String) {
    parse(s, &text);
}

#[then("their types are datetime, float, bool, string and int")]
fn their_types(s: &mut Steps) {
    assert!(s.error.is_none(), "{:?}", s.error);
    let types: Vec<ValType> = s.facets.iter().map(|f| f.val_type()).collect();
    assert_eq!(
        types,
        [
            ValType::Datetime,
            ValType::Float,
            ValType::Bool,
            ValType::String,
            ValType::Int
        ]
    );
}

#[then(regex = r"^the facet `(\w+)` holds the int (\d+)$")]
fn the_facet_holds(s: &mut Steps, key: String, n: i64) {
    let f = s.facets.iter().find(|f| f.key == key).expect("facet");
    assert_eq!(val_for(f).expect("val").value(), Some(&Value::Int(n)));
}

#[then(regex = r#"^parsing fails with "(.+)"$"#)]
fn parsing_fails(s: &mut Steps, msg: String) {
    assert_eq!(s.error.as_ref().map(ToString::to_string), Some(msg));
}

#[when(regex = r"^a facet list naming `(\w+)` twice is sorted and validated$")]
fn repeated_key(s: &mut Steps, key: String) {
    s.facets = [key.as_str(), "close", key.as_str()]
        .iter()
        .map(|k| api::Facet {
            key: k.to_string(),
            ..Default::default()
        })
        .collect();
    s.error = sort_and_validate(&mut s.facets).err();
}

#[then(regex = r#"^it fails with "(.+)"$"#)]
fn it_fails(s: &mut Steps, msg: String) {
    assert_eq!(s.error.as_ref().map(ToString::to_string), Some(msg));
}

fn main() {
    futures::executor::block_on(Steps::cucumber().fail_on_skipped().run_and_exit(
        ultragraphdb_test_bdd::features_dir("ultragraphdb_core_facets"),
    ));
}
