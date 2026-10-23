/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::HashMap;

use serde::Deserialize;
use ultragraphdb_core_dql::NQuad;
use ultragraphdb_proto_dgraph::api;
use ultragraphdb_proto_dgraph::api::value::Val;
use ultragraphdb_proto_pb::pb;

#[derive(Debug, Deserialize)]
struct GValue {
    kind: String,
    str: String,
    int: i64,
    bool: bool,
    float: String,
    bytes: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct GFacet {
    key: String,
    value: String,
    val_type: i32,
    alias: String,
}

#[derive(Debug, Deserialize)]
struct GNQuad {
    subject: String,
    predicate: String,
    object_id: String,
    object_value: Option<GValue>,
    lang: String,
    facets: Vec<GFacet>,
    namespace: u64,
}

#[derive(Debug, Deserialize, PartialEq)]
struct GEdge {
    entity: u64,
    attr: String,
    value: String,
    value_type: i32,
    value_id: u64,
    lang: String,
    op: i32,
    facets: Vec<GFacet>,
    namespace: u64,
}

#[derive(Debug, Deserialize)]
struct GOutcome {
    edge: Option<GEdge>,
    err: String,
}

#[derive(Debug, Deserialize)]
struct Case {
    nquad: GNQuad,
    new_to_uid: HashMap<String, u64>,
    to_edge_using: GOutcome,
    to_delete_pred_edge: GOutcome,
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn value(v: &GValue) -> api::Value {
    let b = unhex(&v.bytes);
    let val = match v.kind.as_str() {
        "default" => Some(Val::DefaultVal(v.str.clone())),
        "str" => Some(Val::StrVal(v.str.clone())),
        "password" => Some(Val::PasswordVal(v.str.clone())),
        "int" => Some(Val::IntVal(v.int)),
        "bool" => Some(Val::BoolVal(v.bool)),
        "double" => Some(Val::DoubleVal(v.float.parse().unwrap())),
        "bytes" => Some(Val::BytesVal(b)),
        "geo" => Some(Val::GeoVal(b)),
        "datetime" => Some(Val::DatetimeVal(b)),
        "bigfloat" => Some(Val::BigfloatVal(b)),
        "vfloat32" => Some(Val::Vfloat32Val(b)),
        "date" => Some(Val::DateVal(b)),
        "uidval" => Some(Val::UidVal(v.int as u64)),
        _ => None,
    };
    api::Value { val }
}

fn nquad(n: &GNQuad) -> NQuad {
    NQuad::new(api::NQuad {
        subject: n.subject.clone(),
        predicate: n.predicate.clone(),
        object_id: n.object_id.clone(),
        object_value: n.object_value.as_ref().map(value),
        lang: n.lang.clone(),
        facets: n
            .facets
            .iter()
            .map(|f| api::Facet {
                key: f.key.clone(),
                value: unhex(&f.value),
                val_type: f.val_type,
                tokens: Vec::new(),
                alias: f.alias.clone(),
            })
            .collect(),
        namespace: n.namespace,
    })
}

fn edge(e: &pb::DirectedEdge) -> GEdge {
    GEdge {
        entity: e.entity,
        attr: e.attr.clone(),
        value: hex(&e.value),
        value_type: e.value_type,
        value_id: e.value_id,
        lang: e.lang.clone(),
        op: e.op,
        facets: e
            .facets
            .iter()
            .map(|f| GFacet {
                key: f.key.clone(),
                value: hex(&f.value),
                val_type: f.val_type,
                alias: f.alias.clone(),
            })
            .collect(),
        namespace: e.namespace,
    }
}

/// The error text with runs of spaces as one: Go's protobuf text puts a random second space between fields.
fn stable_prefix(err: &str) -> String {
    let mut out = err.to_string();
    while out.contains("  ") {
        out = out.replace("  ", " ");
    }
    out
}

fn check(
    what: &str,
    n: &GNQuad,
    want: &GOutcome,
    got: Result<pb::DirectedEdge, ultragraphdb_core_dql::DqlError>,
) {
    // Go dereferences a missing object value and panics; there is no answer to compare.
    if want.err == "panic" {
        return;
    }
    match (want.edge.as_ref(), got) {
        (Some(w), Ok(g)) => assert_eq!(*w, edge(&g), "{what} {n:?}"),
        (None, Err(e)) => assert_eq!(
            stable_prefix(&want.err),
            stable_prefix(&e.to_string()),
            "{what} {n:?}"
        ),
        (w, g) => panic!("{what} {n:?}: want {w:?} / {:?}, got {g:?}", want.err),
    }
}

/// N-Quads of every value type, subjects and objects as uids, blank nodes and missing ones, into edges as
/// Dgraph's `ToEdgeUsing` and `ToDeletePredEdge` convert them.
#[test]
fn test_golden_edges() {
    let path = ultragraphdb_test_bdd::testdata_dir().join("golden/dql_edges.json");
    let golden = ultragraphdb_test_bdd::load_golden::<Case>(&path);
    assert!(golden.cases().len() > 100);
    for c in golden.cases() {
        let nq = nquad(&c.nquad);
        check(
            "to_edge_using",
            &c.nquad,
            &c.to_edge_using,
            nq.to_edge_using(&c.new_to_uid),
        );
        check(
            "to_delete_pred_edge",
            &c.nquad,
            &c.to_delete_pred_edge,
            nq.to_delete_pred_edge(),
        );
    }
}

#[test]
fn test_value_nquad_becomes_value_edge() {
    let nq = NQuad::new(api::NQuad {
        subject: "0x1".into(),
        predicate: "name".into(),
        object_value: Some(api::Value {
            val: Some(Val::DefaultVal("A".into())),
        }),
        lang: "en".into(),
        ..api::NQuad::default()
    });
    let e = nq.to_edge_using(&HashMap::new()).unwrap();
    assert_eq!(e.entity, 1);
    assert_eq!(e.attr, "name");
    assert_eq!(e.value, b"A");
    assert_eq!(e.value_type, pb::posting::ValType::Default as i32);
    assert_eq!(e.lang, "en");
    assert_eq!(e.op, pb::directed_edge::Op::Set as i32);
}
