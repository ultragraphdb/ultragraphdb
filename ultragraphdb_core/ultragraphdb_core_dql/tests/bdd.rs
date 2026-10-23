/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::HashMap;

use cucumber::{World, then, when};
use serde::Deserialize;
use serde_json::Value as Json;
use ultragraphdb_core_dql::{
    DqlError, NQuad, ParseResult, Request, parse, parse_dql, parse_mutation,
};
use ultragraphdb_core_types::{TypeId, Value};
use ultragraphdb_proto_dgraph::api;
use ultragraphdb_proto_pb::pb;
use ultragraphdb_test_bdd::Golden;

// Shared with the unit tests, which use all of it.
#[allow(dead_code)]
#[path = "parser/tree_json.rs"]
mod tree_json;

#[derive(Debug, Default, World)]
struct Steps {
    parsed: Option<Result<ParseResult, DqlError>>,
    request: Option<Result<api::Request, DqlError>>,
    edge: Option<pb::DirectedEdge>,
    mismatches: Vec<String>,
    compared: usize,
}

impl Steps {
    fn error(&self) -> String {
        match (&self.parsed, &self.request) {
            (Some(Err(e)), _) | (_, Some(Err(e))) => e.to_string(),
            other => panic!("expected an error, got {other:?}"),
        }
    }

    fn result(&self) -> &ParseResult {
        match &self.parsed {
            Some(Ok(res)) => res,
            other => panic!("expected a parse result, got {other:?}"),
        }
    }
}

#[derive(Debug, Deserialize)]
struct Outcome {
    ok: Json,
    err: String,
    is_err: bool,
    stable: bool,
}

#[derive(Debug, Deserialize)]
struct Case {
    input: String,
    variables: Option<HashMap<String, String>>,
    parse: Outcome,
}

fn golden() -> Golden<Case> {
    let path = ultragraphdb_test_bdd::testdata_dir().join("golden/dql_parse.json");
    ultragraphdb_test_bdd::load_golden::<Case>(&path)
}

fn request(c: &Case) -> Request {
    match &c.variables {
        Some(vars) => Request::with_variables(c.input.clone(), vars.clone()),
        None => Request::new(c.input.clone()),
    }
}

#[when("each query of the golden DQL corpus is parsed")]
fn golden_trees(s: &mut Steps) {
    for c in golden()
        .cases()
        .iter()
        .filter(|c| c.parse.stable && !c.parse.is_err)
    {
        s.compared += 1;
        match parse(&request(c)) {
            Ok(res) if tree_json::result(&res) == c.parse.ok => {}
            other => s.mismatches.push(format!("{:?}: {other:?}", c.input)),
        }
    }
}

#[then("the tree, serialized as JSON, equals the tree Dgraph's parser produced")]
fn golden_trees_equal(s: &mut Steps) {
    assert!(s.compared > 500, "only {} trees compared", s.compared);
    assert!(
        s.mismatches.is_empty(),
        "{} trees differ: {:?}",
        s.mismatches.len(),
        s.mismatches.first()
    );
}

#[when("the ported cases of Dgraph's dql/parser_test.go run")]
fn golden_outcomes(s: &mut Steps) {
    for c in golden().cases().iter().filter(|c| c.parse.stable) {
        s.compared += 1;
        let ok = match parse(&request(c)) {
            Ok(res) => !c.parse.is_err && tree_json::result(&res) == c.parse.ok,
            Err(e) => c.parse.is_err && e.to_string() == c.parse.err,
        };
        if !ok {
            s.mismatches.push(c.input.clone());
        }
    }
}

#[then("every case gives Dgraph's result or Dgraph's error text")]
fn golden_outcomes_equal(s: &mut Steps) {
    assert!(s.compared > 1500, "only {} cases compared", s.compared);
    assert!(
        s.mismatches.is_empty(),
        "{} cases differ: {:?}",
        s.mismatches.len(),
        s.mismatches.first()
    );
}

#[when(expr = "the query {string} is parsed")]
fn parse_query(s: &mut Steps, query: String) {
    s.parsed = Some(parse(&Request::new(query)));
}

#[when(expr = "the query {string} is parsed with $a = {string}")]
fn parse_query_with_a(s: &mut Steps, query: String, a: String) {
    let vars = HashMap::from([("$a".to_string(), a)]);
    s.parsed = Some(parse(&Request::with_variables(query, vars)));
}

#[when(expr = "the request {string} is parsed as DQL")]
fn parse_as_dql(s: &mut Steps, req: String) {
    s.request = Some(parse_dql(&req));
}

#[when(expr = "the request {string} is parsed as a mutation")]
fn parse_as_mutation(s: &mut Steps, req: String) {
    s.request = Some(parse_mutation(&req));
}

#[when("an upsert block whose mutation carries a condition split across lines is parsed as DQL")]
fn parse_conditional_upsert(s: &mut Steps) {
    let query = "upsert {
  query {
    me(func: eq(email, \"someone@gmail.com\")) {
      v as uid
    }
  }

  mutation @if(eq(len(m), 1)
        AND
        gt(len(f), 0)) {
    set {
      uid(v) <name> \"Some One\" .
    }
  }
}";
    s.request = Some(parse_dql(query));
}

#[then(expr = "parsing fails with {string}")]
fn fails_with(s: &mut Steps, want: String) {
    let err = s.error();
    assert!(
        err.contains(&want),
        "error {err:?} does not contain {want:?}"
    );
}

#[then(expr = "the error names {string}")]
fn error_names(s: &mut Steps, name: String) {
    let err = s.error();
    assert!(
        err.contains(&format!("Used:[{name}]")),
        "error {err:?} does not name {name:?}"
    );
}

#[then(expr = "the parameter in the math expression is the int {int}")]
fn math_param(s: &mut Steps, want: i64) {
    let res = s.result();
    let c = res.query()[0].children()[1]
        .math_exp()
        .expect("math")
        .child()[1]
        .const_val();
    assert_eq!(c.tid(), TypeId::INT);
    assert_eq!(c.value(), Some(&Value::Int(want)));
}

#[then(
    expr = "the languages of the predicate {string} are {string}, {string}, {string} in that order"
)]
fn languages(s: &mut Steps, pred: String, l1: String, l2: String, l3: String) {
    let res = s.result();
    let child = res.query()[0]
        .children()
        .iter()
        .find(|c| c.attr() == pred)
        .expect("predicate");
    assert_eq!(child.langs(), [l1, l2, l3]);
}

#[then(expr = "the mutation's condition is the text {string} as written")]
fn condition(s: &mut Steps, want: String) {
    let Some(Ok(req)) = &s.request else {
        panic!("expected a request, got {:?}", s.request);
    };
    assert_eq!(req.mutations[0].cond, want.replace("\\n", "\n"));
}

#[when("the N-Quad <0x1> <name> \"A\"@en . is converted into a directed edge")]
fn convert_nquad(s: &mut Steps) {
    let nq = NQuad::new(api::NQuad {
        subject: "0x1".into(),
        predicate: "name".into(),
        object_value: Some(api::Value {
            val: Some(api::value::Val::DefaultVal("A".into())),
        }),
        lang: "en".into(),
        ..api::NQuad::default()
    });
    s.edge = Some(nq.to_edge_using(&HashMap::new()).expect("edge"));
}

#[then(
    "the edge has entity 1, attribute \"name\", value bytes \"A\" of type default, language \"en\" and operation SET"
)]
fn value_edge(s: &mut Steps) {
    let e = s.edge.as_ref().expect("edge");
    assert_eq!(e.entity, 1);
    assert_eq!(e.attr, "name");
    assert_eq!(e.value, b"A");
    assert_eq!(e.value_type, pb::posting::ValType::Default as i32);
    assert_eq!(e.lang, "en");
    assert_eq!(e.op, pb::directed_edge::Op::Set as i32);
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_test_bdd::features_dir()),
    );
}
