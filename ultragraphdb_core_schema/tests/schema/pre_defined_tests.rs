/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, schema/schema_test.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_schema::{
    check_and_modify_pre_def_predicate, compare_schema_updates, is_pre_def_type_changed,
    parse_with_namespace,
};
use ultragraphdb_proto_pb::pb;

use crate::golden_json::{compare_updates, predef_cases};

fn su(pred: &str, value_type: pb::posting::ValType, count: bool, upsert: bool) -> pb::SchemaUpdate {
    let mut u = pb::SchemaUpdate {
        predicate: pred.to_string(),
        count,
        upsert,
        ..Default::default()
    };
    u.set_value_type(value_type);
    u
}

#[test]
fn test_compare_schema_updates() {
    use pb::posting::ValType::{Default, String};
    let tests: [(&str, pb::SchemaUpdate, pb::SchemaUpdate, Vec<&str>); 4] = [
        (
            "No changes",
            su("name", String, true, false),
            su("name", String, true, false),
            vec![],
        ),
        (
            "Predicate changed",
            su("name", String, true, false),
            su("age", String, true, false),
            vec!["Predicate"],
        ),
        (
            "Multiple fields changed",
            su("name", String, true, false),
            su("age", String, false, false),
            vec!["Predicate", "Count"],
        ),
        (
            "Unchanged and changed fields",
            su("name", Default, true, false),
            su("name", Default, true, true),
            vec!["Upsert"],
        ),
    ];
    for (name, original, update, expected) in tests {
        assert_eq!(
            compare_schema_updates(&original, &update),
            expected,
            "{name}"
        );
    }
}

#[test]
fn compare_schema_updates_names_every_field_in_go_order() {
    let original = pb::SchemaUpdate::default();
    let update = pb::SchemaUpdate {
        predicate: "p".to_string(),
        value_type: 9,
        directive: 1,
        tokenizer: vec!["exact".to_string()],
        count: true,
        list: true,
        upsert: true,
        lang: true,
        unique: true,
        non_nullable: true,
        non_nullable_list: true,
        object_type_name: "T".to_string(),
        no_conflict: true,
        index_specs: vec![pb::VectorIndexSpec::default()],
    };
    assert_eq!(
        compare_schema_updates(&original, &update),
        [
            "Predicate",
            "ValueType",
            "Directive",
            "Tokenizer",
            "Count",
            "List",
            "Upsert",
            "Lang",
            "Unique",
            "NonNullable",
            "NonNullableList",
            "ObjectTypeName",
            "NoConflict",
            "IndexSpecs"
        ]
    );
}

#[test]
fn pre_defined_checks_are_dgraphs() {
    let golden = predef_cases();
    assert!(!golden.cases().is_empty());
    for c in golden.cases() {
        let ctx = format!("{:?} in namespace {}", c.input, c.namespace);
        let parsed = parse_with_namespace(&c.input, c.namespace).expect("parses");
        let (mut preds, types) = parsed.into_parts();
        let changed: Vec<bool> = preds
            .iter_mut()
            .map(check_and_modify_pre_def_predicate)
            .collect();
        assert_eq!(changed, c.pred_changed, "{ctx}");
        compare_updates(&preds, &c.preds_after).unwrap_or_else(|m| panic!("{ctx}: {m}"));
        let type_changed: Vec<bool> = types.iter().map(is_pre_def_type_changed).collect();
        assert_eq!(type_changed, c.type_changed, "{ctx}");
    }
}

#[test]
fn dgraph_xid_without_unique_is_upgraded() {
    let parsed =
        parse_with_namespace("dgraph.xid: string @index(exact) @upsert .", 0).expect("parses");
    let (mut preds, _) = parsed.into_parts();
    assert!(!check_and_modify_pre_def_predicate(&mut preds[0]));
    assert!(preds[0].unique);
}
