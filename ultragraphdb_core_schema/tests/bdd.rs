/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use cucumber::{World, then, when};
use ultragraphdb_core_keys::parse_namespace;
use ultragraphdb_core_schema::{
    ParsedSchema, SchemaError, SchemaMode, SchemaState, complete_initial_schema,
    complete_initial_types, parse, parse_with_namespace,
};
use ultragraphdb_core_types::TypeId;
use ultragraphdb_proto_pb::pb;

// Shared with the unit tests, which use all of it.
#[allow(dead_code)]
#[path = "golden_json.rs"]
mod golden_json;

#[derive(Debug, Default, World)]
struct Steps {
    parsed: Option<Result<ParsedSchema, SchemaError>>,
    state: SchemaState,
    mismatches: Vec<String>,
    compared: usize,
    initial: Vec<(u64, Vec<pb::SchemaUpdate>, Vec<pb::TypeUpdate>)>,
}

impl Steps {
    fn result(&self) -> &ParsedSchema {
        match &self.parsed {
            Some(Ok(res)) => res,
            other => panic!("expected a parse result, got {other:?}"),
        }
    }

    fn only_pred(&self) -> &pb::SchemaUpdate {
        let preds = self.result().preds();
        assert_eq!(preds.len(), 1, "expected one predicate, got {preds:?}");
        &preds[0]
    }
}

#[when(expr = "the schema {string} is parsed")]
fn parse_schema(s: &mut Steps, text: String) {
    s.parsed = Some(parse(&text));
}

#[when(expr = "the schema {string} is parsed for namespace {int}")]
fn parse_schema_ns(s: &mut Steps, text: String, ns: u64) {
    s.parsed = Some(parse_with_namespace(&text, ns));
}

#[when(expr = "a type declaring the field {string} twice is parsed")]
fn parse_duplicate_fields(s: &mut Steps, field: String) {
    s.parsed = Some(parse(&format!(
        "type Person {{\n  {field}\n  {field}\n}}\n"
    )));
}

#[then(expr = "parsing fails with {string}")]
fn parsing_fails(s: &mut Steps, msg: String) {
    match &s.parsed {
        Some(Err(e)) => {
            let text = e.to_string();
            assert!(text.contains(&msg), "error {text:?} lacks {msg:?}");
        }
        other => panic!("expected an error, got {other:?}"),
    }
}

#[then(expr = "the predicate is {string} of value type {int} and a list")]
fn pred_is_list(s: &mut Steps, pred: String, value_type: i32) {
    let p = s.only_pred();
    assert_eq!(p.predicate, pred);
    assert_eq!(p.value_type, value_type);
    assert!(p.list);
}

#[then(regex = r#"^the predicate is "(.*)" in namespace (0x[0-9a-f]+|\d+)$"#)]
fn pred_in_namespace(s: &mut Steps, pred: String, ns: String) {
    let want = match ns.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).expect("hex namespace"),
        None => ns.parse().expect("decimal namespace"),
    };
    let p = s.only_pred();
    assert_eq!(p.predicate, pred);
    assert_eq!(parse_namespace(&p.predicate), want);
}

#[then(expr = "the update carries an hnsw vector index with metric {string} and exponent {string}")]
fn hnsw_spec(s: &mut Steps, metric: String, exponent: String) {
    let p = s.only_pred();
    assert_eq!(p.directive(), pb::schema_update::Directive::Index);
    assert_eq!(p.index_specs.len(), 1);
    let spec = &p.index_specs[0];
    assert_eq!(spec.name, "hnsw");
    let opts: Vec<(&str, &str)> = spec
        .options
        .iter()
        .map(|o| (o.key.as_str(), o.value.as_str()))
        .collect();
    assert_eq!(
        opts,
        [("metric", metric.as_str()), ("exponent", exponent.as_str())]
    );
}

#[when("each schema of the golden schema corpus is parsed")]
fn golden_corpus(s: &mut Steps) {
    for c in golden_json::parse_cases().cases() {
        s.compared += 1;
        for (label, got, want) in [
            ("parse", parse(&c.input), &c.parse),
            ("ns5", parse_with_namespace(&c.input, 5), &c.ns5),
        ] {
            if let Err(m) = golden_json::compare_outcome(&got, want) {
                s.mismatches.push(format!("{label} {:?}: {m}", c.input));
            }
        }
    }
}

#[then("the updates equal Dgraph's")]
fn updates_equal(s: &mut Steps) {
    assert!(s.compared > 0, "no golden cases compared");
    assert!(
        s.mismatches.is_empty(),
        "{} of {} cases differ:\n{}",
        s.mismatches.len(),
        s.compared,
        s.mismatches.join("\n")
    );
}

#[when(expr = "the schema {string} is loaded into the state")]
fn load_state(s: &mut Steps, text: String) {
    s.state.parse_text(&text).expect("schema loads");
}

#[then(expr = "the state reports type string, tokenizer {string} and upsert true for {string}")]
fn state_reports(s: &mut Steps, tokenizer: String, pred: String) {
    assert_eq!(s.state.type_of(&pred).expect("defined"), TypeId::STRING);
    assert_eq!(
        s.state.tokenizer_names(SchemaMode::Query, &pred),
        [tokenizer]
    );
    assert!(s.state.has_upsert(&pred));
}

#[then(expr = "the update of {string} carries unique true")]
fn update_unique(s: &mut Steps, pred: String) {
    let update = s.state.get(SchemaMode::Query, &pred).expect("defined");
    assert!(update.unique);
}

#[when("the complete initial schema and types are taken for namespaces 0 and 5")]
fn take_initial(s: &mut Steps) {
    s.initial = [0, 5]
        .into_iter()
        .map(|ns| (ns, complete_initial_schema(ns), complete_initial_types(ns)))
        .collect();
}

#[then(
    expr = "they equal Dgraph's, with {string}, the ACL entries and the {string} entries in namespace 0"
)]
fn initial_equal(s: &mut Steps, dgraph_type: String, graphql: String) {
    let golden = golden_json::initial_cases();
    for (ns, schema, types) in &s.initial {
        let want = golden
            .cases()
            .iter()
            .find(|c| c.namespace == *ns)
            .expect("golden namespace");
        golden_json::compare_updates(schema, &want.complete_schema).expect("initial schema");
        golden_json::compare_types(types, &want.complete_types).expect("initial types");
    }
    let (_, schema0, types0) = &s.initial[0];
    let preds: Vec<&str> = schema0.iter().map(|u| u.predicate.as_str()).collect();
    assert!(preds.contains(&format!("0-{dgraph_type}").as_str()));
    let prefix = format!("0-{}", graphql.trim_end_matches('*'));
    assert!(preds.iter().any(|p| p.starts_with(&prefix)));
    assert!(types0.iter().any(|t| t.type_name.starts_with(&prefix)));
    for acl in [
        "0-dgraph.xid",
        "0-dgraph.password",
        "0-dgraph.user.group",
        "0-dgraph.acl.rule",
        "0-dgraph.rule.predicate",
        "0-dgraph.rule.permission",
    ] {
        assert!(preds.contains(&acl), "missing {acl}");
    }
    assert!(types0.iter().any(|t| t.type_name == "0-dgraph.type.User"));
}

#[then(expr = "namespace 5's lack {string} and {string}")]
fn ns5_lacks(s: &mut Steps, a: String, b: String) {
    let (ns, schema5, types5) = &s.initial[1];
    assert_eq!(*ns, 5);
    for name in [a, b] {
        let pred = format!("5-{name}");
        assert!(
            schema5.iter().all(|u| u.predicate != pred),
            "{pred} present"
        );
        assert!(
            types5
                .iter()
                .flat_map(|t| &t.fields)
                .all(|f| f.predicate != pred),
            "{pred} present in a type"
        );
    }
    assert!(types5.iter().all(|t| t.type_name != "5-dgraph.namespace"));
}

fn main() {
    futures::executor::block_on(Steps::cucumber().fail_on_skipped().run_and_exit(
        ultragraphdb_test_bdd::features_dir("ultragraphdb_core_schema"),
    ));
}
