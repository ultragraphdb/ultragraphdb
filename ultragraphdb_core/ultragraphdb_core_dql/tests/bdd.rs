/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::HashMap;

use cucumber::{World, then, when};
use ultragraphdb_core_dql::{
    DqlError, NQuad, ParseResult, Request, delete_predicate_edge, parse, parse_dql, parse_mutation,
};
use ultragraphdb_core_types::{TypeId, Value};
use ultragraphdb_proto_dgraph::api;
use ultragraphdb_proto_pb::pb;

use crate::golden_parse::{Compared, check_parse, deviations_listed, golden, request};

// Shared with the unit tests, which use all of it.
#[allow(dead_code)]
#[path = "parser/tree_json.rs"]
mod tree_json;

// Shared with the unit tests, which also compare the mutation and DQL outcomes.
#[allow(dead_code)]
#[path = "parser/golden_parse.rs"]
mod golden_parse;

#[path = "parser/parser_deviations.rs"]
mod parser_deviations;

#[derive(Debug, Default, World)]
struct Steps {
    parsed: Option<Result<ParseResult, DqlError>>,
    request: Option<Result<api::Request, DqlError>>,
    edge: Option<pb::DirectedEdge>,
    conversion: Option<Result<pb::DirectedEdge, DqlError>>,
    object_error: Option<DqlError>,
    mismatches: Vec<String>,
    compared: usize,
    deviations: usize,
    /// Whether the query was parsed on a thread with a 256 KiB stack.
    small_stack: bool,
    /// The functions a flat filter chain joins.
    terms: usize,
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

#[when("each query of the golden DQL corpus is parsed")]
fn golden_trees(s: &mut Steps) {
    for c in golden().cases().iter().filter(|c| c.parse.has_tree()) {
        s.compared += 1;
        match parse(&request(c)) {
            Ok(res) if c.parse.admits(&Ok(tree_json::result(&res))) => {}
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
    for c in golden().cases() {
        s.compared += 1;
        match check_parse(c) {
            Ok(Compared::AsGo) => {}
            Ok(Compared::Deviation) => s.deviations += 1,
            Err(e) => s.mismatches.push(e),
        }
    }
}

#[then("every case gives Dgraph's result or Dgraph's error text")]
fn golden_outcomes_equal(s: &mut Steps) {
    assert!(s.compared > 2000, "only {} cases compared", s.compared);
    assert!(
        s.mismatches.is_empty(),
        "{} cases differ: {:?}",
        s.mismatches.len(),
        s.mismatches.first()
    );
    assert_eq!(
        s.deviations,
        deviations_listed(),
        "every deviation names a golden case"
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

#[when(expr = "the query {string} is parsed with the variables {string}")]
fn parse_query_with_variables(s: &mut Steps, query: String, variables: String) {
    let vars = variables
        .split(", ")
        .filter_map(|kv| kv.split_once(": "))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    s.parsed = Some(parse(&Request::with_variables(query, vars)));
}

#[then(expr = "the error holds {string} and {string}, in that order")]
fn error_holds_both(s: &mut Steps, first: String, second: String) {
    assert_eq!(s.error(), format!("{first}\n{second}"));
}

/// `{ q(func: uid(1)) { a { a { ... a } } } }`, its innermost predicate inside `depth` braces: a tree of `depth`
/// levels.
fn nested_query(depth: usize) -> String {
    let inner = depth.saturating_sub(2);
    format!(
        "{{ q(func: uid(1)) {{ {}a{} }} }}",
        "a { ".repeat(inner),
        " }".repeat(inner)
    )
}

/// Parses the query on a thread with a 256 KiB stack.
fn parse_on_small_stack(query: String) -> Result<ParseResult, DqlError> {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || parse(&Request::new(query)))
        .expect("thread")
        .join()
        .expect("parsed")
}

#[when(regex = r"^a query whose selection blocks nest ([\d,]+) levels deep is parsed$")]
fn parse_nested(s: &mut Steps, depth: String) {
    let depth = depth.replace(',', "").parse().expect("a depth");
    s.parsed = Some(parse_on_small_stack(nested_query(depth)));
    s.small_stack = true;
}

#[when(
    regex = r#"^a filter of ([\d,]+) "(\w+)" functions joined by "(\w+)" is parsed, cloned and dropped$"#
)]
fn parse_flat_filter(s: &mut Steps, terms: String, function: String, op: String) {
    let terms: usize = terms.replace(',', "").parse().expect("a count");
    let filter = vec![format!("{function}(a, 1)"); terms].join(&format!(" {op} "));
    let query = format!("{{ q(func: uid(1)) @filter({filter}) {{ a }} }}");
    s.parsed = Some(
        std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(move || {
                let res = parse(&Request::new(query));
                drop(res.clone());
                res
            })
            .expect("thread")
            .join()
            .expect("parsed"),
    );
    s.small_stack = true;
    s.terms = terms;
}

#[then("it parses into one filter tree and nothing overflows the stack")]
fn one_filter_tree(s: &mut Steps) {
    assert!(s.small_stack, "the query was parsed on the default stack");
    let res = s.result();
    assert_eq!(res.query().len(), 1);
    let filter = res.query()[0].filter().expect("a filter");
    // Counts the functions at the leaves, without recursion.
    let mut leaves = 0;
    let mut todo = vec![filter];
    while let Some(node) = todo.pop() {
        if node.func().is_some() {
            leaves += 1;
        }
        todo.extend(node.child());
    }
    assert_eq!(leaves, s.terms);
}

#[then(expr = "parsing fails with {string} and nothing overflows the stack")]
fn fails_on_small_stack(s: &mut Steps, want: String) {
    assert!(s.small_stack, "the query was parsed on the default stack");
    assert_eq!(s.error(), want);
}

#[then(regex = r"^it parses into a tree ([\d,]+) levels deep$")]
fn tree_depth(s: &mut Steps, depth: String) {
    let want: usize = depth.replace(',', "").parse().expect("a depth");
    let res = s.result();
    assert_eq!(res.query().len(), 1);
    // Walks down the only child at each level, without recursion.
    let mut levels = 1;
    let mut node = &res.query()[0];
    while let [child] = node.children() {
        levels += 1;
        node = child;
    }
    assert!(node.children().is_empty(), "a level with several children");
    assert_eq!(levels, want);
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

#[when(expr = "the N-Quad with subject {string}, predicate {string} and no object becomes an edge")]
fn convert_objectless(s: &mut Steps, subject: String, predicate: String) {
    let nq = NQuad::new(api::NQuad {
        subject,
        predicate,
        ..api::NQuad::default()
    });
    s.object_error = nq.object().err();
    s.conversion = Some(nq.to_edge_using(&HashMap::new()));
}

#[then(expr = "the conversion fails with {string} and nothing panics")]
fn conversion_fails(s: &mut Steps, message: String) {
    let err = s.object_error.as_ref().expect("the N-Quad has no object");
    assert!(err.to_string().starts_with(&message), "{err}");
    let err = s
        .conversion
        .take()
        .expect("converted")
        .expect_err("the N-Quad has no object");
    assert!(err.to_string().starts_with(&message), "{err}");
}

#[when(expr = "the deletion edge of predicate {string} in namespace {int} is built")]
fn deletion_edge(s: &mut Steps, predicate: String, namespace: u64) {
    s.edge = Some(delete_predicate_edge(&predicate, namespace));
}

#[then(expr = "its value is {string}, its type is string and its operation is DEL")]
fn star_edge(s: &mut Steps, value: String) {
    let e = s.edge.as_ref().expect("edge");
    assert_eq!(e.value, value.as_bytes());
    assert_eq!(e.value_type, pb::posting::ValType::String as i32);
    assert_eq!(e.op, pb::directed_edge::Op::Del as i32);
    assert_eq!((e.entity, e.attr.as_str(), e.namespace), (0, "name", 0));
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_test_bdd::features_dir()),
    );
}
