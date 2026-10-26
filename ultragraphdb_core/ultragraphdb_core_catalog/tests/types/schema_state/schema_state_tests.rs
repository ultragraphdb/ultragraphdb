/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, schema/parse_test.go (the cases loading the state through ParseBytes). Changed for
 * ultragraphdb: translated to Rust; the state is a value rather than a package global.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::HashSet;

use ultragraphdb_core_keys::{attr_in_root_namespace, namespace_attr};
use ultragraphdb_core_catalog::{SchemaMode, SchemaState};
use ultragraphdb_core_tok::{IDENT_EXACT, IDENT_TERM, VEC_DEAD, VEC_ENTRY, VEC_KEYWORD};
use ultragraphdb_core_types::TypeId;
use ultragraphdb_proto_pb::pb;
use ultragraphdb_proto_pb::pb::schema_update::Directive;

const Q: SchemaMode = SchemaMode::Query;
const W: SchemaMode = SchemaMode::Write;

fn root(attr: &str) -> String {
    attr_in_root_namespace(attr)
}

fn su(pred: &str, value_type: i32) -> pb::SchemaUpdate {
    pb::SchemaUpdate {
        predicate: pred.to_string(),
        value_type,
        ..Default::default()
    }
}

fn check_schema(state: &SchemaState, expected: &[pb::SchemaUpdate]) {
    assert_eq!(state.predicates().len(), expected.len());
    for want in expected {
        let got = state
            .get(Q, &want.predicate)
            .unwrap_or_else(|| panic!("{} not found", want.predicate));
        assert_eq!(&got, want);
    }
}

fn loaded(s: &str) -> SchemaState {
    let state = SchemaState::new();
    state.parse_text(s).expect("schema loads");
    state
}

const SCHEMA_VAL: &str = r#"
age:int .

name: string .
 address: string .
<http://scalar.com/helloworld/> : string .
amount: bigfloat .
coordinates: float32vector .
indexvector: float32vector @index(hnsw(metric:"euclidean")) .
"#;

#[test]
fn test_schema() {
    let state = loaded(SCHEMA_VAL);
    let mut indexvector = su(&root("indexvector"), 12);
    indexvector.set_directive(Directive::Index);
    indexvector.index_specs = vec![pb::VectorIndexSpec {
        name: "hnsw".to_string(),
        options: vec![pb::OptionPair {
            key: "metric".to_string(),
            value: "euclidean".to_string(),
        }],
    }];
    check_schema(
        &state,
        &[
            su(&root("name"), 9),
            su(&root("address"), 9),
            su(&root("http://scalar.com/helloworld/"), 9),
            su(&root("age"), 2),
            su(&root("amount"), 11),
            su(&root("coordinates"), 12),
            indexvector,
        ],
    );
    assert_eq!(state.type_of(&root("age")).unwrap(), TypeId::INT);
    assert_eq!(state.type_of(&root("amount")).unwrap(), TypeId::BIGFLOAT);
    assert_eq!(state.type_of(&root("coordinates")).unwrap(), TypeId::VFLOAT);
    assert!(state.type_of(&root("agea")).is_err());
}

#[test]
fn test_schema1_error() {
    let state = SchemaState::new();
    assert!(
        state
            .parse_text("\nage:int .\n\nname: string .\naddress: string .\n\n)\n")
            .is_err()
    );
}

#[test]
fn test_schema2_error() {
    assert!(SchemaState::new().parse_text("\nname: ( string\n").is_err());
}

#[test]
fn test_schema3_error() {
    assert!(SchemaState::new().parse_text("\ntest test: int\n").is_err());
}

// Duplicate tokenizers
#[test]
fn test_schema_index_error1() {
    let schema_index_val2 = "
name: string @index(exact, exact) .
address: string @index(term) .
id: id @index(exact, term, exact) .
";
    assert!(SchemaState::new().parse_text(schema_index_val2).is_err());
}

// Object types cant be indexed.
#[test]
fn test_schema_index_error2() {
    let state = SchemaState::new();
    assert!(state.parse_text("\nperson: uid @index .\n").is_err());
    assert!(state.parse_text("\nvalue: default @index .\n").is_err());
    assert!(state.parse_text("\npass: password @index .\n").is_err());
}

// Missing comma.
#[test]
fn test_schema_index_error3() {
    assert!(
        SchemaState::new()
            .parse_text("\nname:string @index(exact term) .\n")
            .is_err()
    );
}

const SCHEMA_INDEX_VAL5: &str = "
age     : int @index(int) .
name    : string @index(exact) @count .
address : string @index(term) .
friend  : [uid] @reverse @count .
";

#[test]
fn test_schema_index_custom() {
    let state = loaded(SCHEMA_INDEX_VAL5);
    let mut name = su(&root("name"), 9);
    name.tokenizer = vec!["exact".to_string()];
    name.set_directive(Directive::Index);
    name.count = true;
    let mut address = su(&root("address"), 9);
    address.tokenizer = vec!["term".to_string()];
    address.set_directive(Directive::Index);
    let mut age = su(&root("age"), 2);
    age.tokenizer = vec!["int".to_string()];
    age.set_directive(Directive::Index);
    let mut friend = su(&root("friend"), 7);
    friend.set_directive(Directive::Reverse);
    friend.count = true;
    friend.list = true;
    check_schema(&state, &[name, address, age, friend]);
    assert!(state.is_indexed(Q, &root("name")));
    assert!(!state.is_reversed(Q, &root("name")));
    assert_eq!(state.tokenizer(Q, &root("age")).unwrap()[0].name(), "int");
}

// Rust-only cases: every accessor of the state, in both modes.

#[test]
fn accessors_answer_for_the_predicate() {
    let state = loaded(
        "
name: string @index(exact, term) @count @upsert @lang .
friend: [uid] @reverse @noconflict .
age: int .
",
    );
    let name = root("name");
    let friend = root("friend");
    let age = root("age");
    assert_eq!(state.tokenizer_names(Q, &name).unwrap(), ["exact", "term"]);
    assert!(state.has_tokenizer(Q, IDENT_TERM, &name).unwrap());
    assert!(!state.has_tokenizer(Q, IDENT_TERM, &age).unwrap());
    assert!(state.has_count(Q, &name) && !state.has_count(Q, &age));
    assert!(state.has_upsert(&name) && !state.has_upsert(&age));
    assert!(state.has_lang(&name) && !state.has_lang(&age));
    assert!(state.is_reversed(Q, &friend) && !state.is_reversed(Q, &name));
    assert!(state.is_list(&friend) && !state.is_list(&name));
    assert!(state.has_no_conflict(&friend) && !state.has_no_conflict(&name));
    assert!(!state.is_indexed(Q, &age));
    {
        let missing = root("nope");
        assert!(state.tokenizer(Q, &missing).unwrap().is_empty());
        assert!(state.tokenizer_names(Q, &missing).unwrap().is_empty());
        assert!(!state.has_count(Q, &missing));
        assert!(!state.has_upsert(&missing));
        assert!(!state.has_lang(&missing));
        assert!(!state.has_no_conflict(&missing));
        assert!(!state.is_list(&missing));
        assert!(!state.is_reversed(Q, &missing));
        assert!(!state.is_indexed(Q, &missing));
        assert!(state.get(Q, &missing).is_none());
        assert!(state.vector_indexes(Q, &missing).is_empty());
        assert!(state.predicates_to_delete(&missing).is_empty());
    }
}

#[test]
fn write_mode_reads_the_mutation_schema_first() {
    let state = loaded("name: string .\n");
    let name = root("name");
    let mut indexed = su(&name, 9);
    indexed.set_directive(Directive::Index);
    indexed.tokenizer = vec!["exact".to_string()];
    indexed.count = true;
    state.set_mut_schema(&name, indexed.clone());
    assert!(state.indexing_in_progress());
    assert_eq!(state.indexing_predicates(), std::slice::from_ref(&name));
    assert!(state.is_indexed(W, &name) && !state.is_indexed(Q, &name));
    assert!(state.has_count(W, &name) && !state.has_count(Q, &name));
    assert!(state.has_tokenizer(W, IDENT_EXACT, &name).unwrap());
    assert!(!state.has_tokenizer(Q, IDENT_EXACT, &name).unwrap());
    assert_eq!(state.get(W, &name), Some(indexed));
    assert_eq!(state.get(Q, &name), Some(su(&name, 9)));
    state.delete_mut_schema(&name);
    assert!(!state.indexing_in_progress());
    assert!(state.indexing_predicates().is_empty());
    assert!(!state.is_indexed(W, &name));

    let friend = root("friend");
    let mut reversed = su(&friend, 7);
    reversed.set_directive(Directive::Reverse);
    state.set_mut_schema(&friend, reversed);
    assert!(state.is_reversed(W, &friend) && !state.is_reversed(Q, &friend));
    // A predicate only the mutation schema holds is not in the state's predicates.
    assert!(state.type_of(&friend).is_err());
}

#[test]
fn vector_indexes_name_the_factory_and_its_options() {
    let state = loaded(r#"v: float32vector @index(hnsw(metric:"cosine", exponent:"4")) ."#);
    let v = root("v");
    let specs = state.factory_create_spec(Q, &v).expect("specs");
    assert_eq!(specs.len(), 1);
    assert_eq!(
        state.vector_indexes(Q, &v),
        [r#"hnsw("exponent":"4","metric":"cosine")"#]
    );
    assert!(state.is_indexed(Q, &v));
    assert_eq!(
        state.predicates_to_delete(&v),
        [
            v.clone(),
            format!("{v}{VEC_ENTRY}"),
            format!("{v}{VEC_KEYWORD}"),
            format!("{v}{VEC_DEAD}")
        ]
    );
    let plain = loaded("v: float32vector .\n");
    assert_eq!(plain.predicates_to_delete(&v), std::slice::from_ref(&v));
    assert!(plain.factory_create_spec(Q, &v).expect("specs").is_empty());
}

#[test]
fn types_and_namespaces() {
    let state = SchemaState::new();
    let parsed = ultragraphdb_core_catalog::parse(
        "[0x1] type Person {\n name\n}\n[0x2] type Animal {\n name\n}\n[0x1] name: string .\n[0x2] name: string .",
    )
    .expect("parses");
    let (preds, types) = parsed.into_parts();
    for p in preds {
        state.set(&p.predicate.clone(), p);
    }
    for t in types {
        state.set_type(&t.type_name.clone(), t);
    }
    let mut type_names = state.types();
    type_names.sort();
    assert_eq!(
        type_names,
        [namespace_attr(1, "Person"), namespace_attr(2, "Animal")]
    );
    assert_eq!(state.namespaces().unwrap(), HashSet::from([1, 2]));
    assert_eq!(
        state
            .get_type(&namespace_attr(1, "Person"))
            .expect("type")
            .fields[0]
            .predicate,
        namespace_attr(1, "name")
    );
    assert!(state.get_type(&namespace_attr(3, "Person")).is_none());

    state.delete_preds_for_ns(1);
    assert_eq!(state.predicates(), [namespace_attr(2, "name")]);
    assert_eq!(state.types(), [namespace_attr(2, "Animal")]);
    assert_eq!(state.namespaces().unwrap(), HashSet::from([2]));

    state.delete_all();
    assert!(state.predicates().is_empty() && state.types().is_empty());
    assert!(state.namespaces().unwrap().is_empty());
}

#[test]
fn parse_text_replaces_the_state() {
    let state = loaded("a: string .\nb: int .\n");
    assert_eq!(state.predicates().len(), 2);
    state.parse_text("c: string .\n").expect("loads");
    assert_eq!(state.predicates(), [root("c")]);
    // A failed load leaves the state empty, as Go's ParseBytes, which drops everything first.
    assert!(state.parse_text("c: foo .\n").is_err());
    assert!(state.predicates().is_empty());
}

#[test]
fn an_unknown_tokenizer_set_directly_is_refused() {
    // Parsing refuses an unknown tokenizer; a schema set directly can hold one, on which Dgraph's assertion exits.
    let state = SchemaState::new();
    let mut update = su(&root("name"), 9);
    update.tokenizer = vec!["bogus".to_string()];
    update.set_directive(Directive::Index);
    state.set(&root("name"), update);
    let err = state
        .tokenizer(Q, &root("name"))
        .expect_err("unknown tokenizer");
    assert_eq!(err.to_string(), "Invalid tokenizer bogus");
    assert!(state.tokenizer_names(Q, &root("name")).is_err());
    assert!(state.has_tokenizer(Q, IDENT_TERM, &root("name")).is_err());
}

#[test]
fn a_type_without_a_namespace_is_refused_or_kept() {
    // Dgraph exits the process on a name whose namespace does not parse.
    let state = SchemaState::new();
    state.set_type(
        "nonamespace",
        pb::TypeUpdate {
            type_name: "nonamespace".to_string(),
            ..Default::default()
        },
    );
    assert!(state.namespaces().is_err());
    state.delete_preds_for_ns(0);
    assert!(state.get_type("nonamespace").is_some());
}

/// What a drop does to memory, the store's keys removed by the drop itself: a predicate leaves with its background
/// schema, a type alone.
#[test]
fn remove_forgets_a_predicate_or_a_type_in_memory_alone() {
    let state = SchemaState::new();
    state.set("0-name", pb::SchemaUpdate::default());
    state.set_mut_schema("0-name", pb::SchemaUpdate::default());
    state.set_type("0-Person", pb::TypeUpdate::default());
    state.remove_predicate("0-name");
    assert!(state.get(SchemaMode::Write, "0-name").is_none());
    assert!(!state.indexing_in_progress());
    assert!(state.get_type("0-Person").is_some());
    state.remove_type("0-Person");
    assert!(state.get_type("0-Person").is_none());
}
