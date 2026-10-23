/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::HashMap;

use serde::Deserialize;
use ultragraphdb_core_dql::{NQuad, Object, delete_predicate_edge};
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

/// The golden outcomes the port cannot express, with the ledger entry (`openspec/notes/dgraph-bugs-found-in-port.md`)
/// and what the port has instead. `ToDeletePredEdge` takes an N-Quad, and panics on one without an object value
/// (ledger 13) and checks only its subject (29); the port's [`delete_predicate_edge`] takes the attribute and the
/// namespace, so no N-Quad form of the golden file reaches it, and it is checked in
/// `delete_predicate_edge_is_dropattrs_edge`.
const DEVIATIONS: &[(&str, &str, &str)] = &[(
    "to_delete_pred_edge of every case",
    "13, 29",
    "not expressible: delete_predicate_edge(attr, namespace)",
)];

/// N-Quads of every value type, subjects and objects as uids, blank nodes and missing ones, into edges as
/// Dgraph's `ToEdgeUsing` converts them; its `ToDeletePredEdge` outcomes are the one [`DEVIATIONS`] entry.
#[test]
fn test_golden_edges() {
    assert_eq!(DEVIATIONS.len(), 1);
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
        // Go panics only in ToDeletePredEdge, the deviation above.
        assert_ne!(c.to_edge_using.err, "panic", "{:?}", c.nquad);
        assert!(c.to_delete_pred_edge.edge.is_some() || !c.to_delete_pred_edge.err.is_empty());
    }
}

/// Go's DropAttr converts `* <attr> "_STAR_ALL"` with a string value; its edge is the one the golden file records
/// for `* name "hello"` with the star's bytes.
#[test]
fn delete_predicate_edge_is_dropattrs_edge() {
    let e = delete_predicate_edge("name", 2);
    assert_eq!(
        edge(&e),
        GEdge {
            entity: 0,
            attr: "name".to_string(),
            value: hex(b"_STAR_ALL"),
            value_type: 9,
            value_id: 0,
            lang: String::new(),
            op: 1,
            facets: Vec::new(),
            namespace: 2,
        }
    );
}

#[test]
fn an_nquad_without_an_object_has_none() {
    // Go's CreateValueEdge dereferences the missing value and panics (ledger 13).
    let nq = NQuad::new(api::NQuad {
        subject: "0x1".into(),
        predicate: "name".into(),
        ..api::NQuad::default()
    });
    let err = nq.object().expect_err("no object");
    assert_eq!(
        err.to_string(),
        r#"Unknown value type for nquad: subject:"0x1" predicate:"name""#
    );
    let with_id = NQuad::new(api::NQuad {
        object_id: "0x2".into(),
        ..nq.nquad().clone()
    });
    assert_eq!(with_id.object().unwrap(), Object::Uid("0x2"));
    let value = api::Value {
        val: Some(Val::IntVal(3)),
    };
    let with_value = NQuad::new(api::NQuad {
        object_value: Some(value.clone()),
        ..with_id.nquad().clone()
    });
    assert_eq!(with_value.object().unwrap(), Object::Value(&value));
    let e = with_value.create_value_edge(1, &value).unwrap();
    assert_eq!((e.entity, e.value_type), (1, 2));
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
