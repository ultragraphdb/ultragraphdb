/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The schema and type entries against those Dgraph's `updateSchema` and `updateType` write to Badger: key, value,
//! meta byte and discard flag, for every predicate and type of the schema corpus and the complete initial schema.

use serde::Deserialize;
use ultragraphdb_core_schema::{
    SchemaErrorEnum, complete_initial_schema, complete_initial_types, parse, schema_entry,
    type_entry,
};
use ultragraphdb_proto_pb::pb;
use ultragraphdb_test_bdd::Golden;
use ultragraphdb_traits_store::Entry;

#[derive(Debug, Deserialize)]
struct EntryJson {
    key: String,
    value: String,
    user_meta: u8,
    discard: bool,
}

#[derive(Debug, Deserialize)]
struct EntriesCase {
    input: String,
    /// The namespace of the complete initial schema and types, or -1 for a parsed input.
    initial: i64,
    preds: Vec<EntryJson>,
    types: Vec<EntryJson>,
}

fn golden() -> Golden<EntriesCase> {
    let path = ultragraphdb_test_bdd::testdata_dir().join("golden/schema_storage_entries.json");
    ultragraphdb_test_bdd::load_golden(&path)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn check(ctx: &str, got: &Entry, want: &EntryJson, ts: u64) {
    assert_eq!(hex(got.key()), want.key, "{ctx}: key");
    assert_eq!(
        hex(got.value().expect("a value, not a deletion")),
        want.value,
        "{ctx}: value"
    );
    assert_eq!(got.meta(), want.user_meta, "{ctx}: meta");
    assert_eq!(got.discard(), want.discard, "{ctx}: discard");
    assert_eq!(got.version(), ts, "{ctx}: version");
}

fn check_all(ctx: &str, preds: &[pb::SchemaUpdate], types: &[pb::TypeUpdate], c: &EntriesCase) {
    assert_eq!(preds.len(), c.preds.len(), "{ctx}: predicates");
    assert_eq!(types.len(), c.types.len(), "{ctx}: types");
    for (p, want) in preds.iter().zip(&c.preds) {
        let entry = schema_entry(p, 7).expect("a namespaced predicate");
        check(&format!("{ctx} {}", p.predicate), &entry, want, 7);
    }
    for (t, want) in types.iter().zip(&c.types) {
        let entry = type_entry(t, 7).expect("a namespaced type");
        check(&format!("{ctx} {}", t.type_name), &entry, want, 7);
    }
}

#[test]
fn entries_are_dgraphs() {
    let golden = golden();
    assert!(golden.cases().len() > 400, "{}", golden.cases().len());
    let mut compared = 0;
    for c in golden.cases() {
        let (preds, types) = if c.initial < 0 {
            parse(&c.input)
                .unwrap_or_else(|e| panic!("{:?} parses in Go: {e}", c.input))
                .into_parts()
        } else {
            let ns = c.initial.unsigned_abs();
            (complete_initial_schema(ns), complete_initial_types(ns))
        };
        check_all(&format!("{:?}", c.input), &preds, &types, c);
        compared += c.preds.len() + c.types.len();
    }
    assert!(compared > 500, "{compared}");
}

#[test]
fn every_entry_is_a_schema_posting_with_the_discard_flag() {
    let golden = golden();
    assert!(
        golden
            .cases()
            .iter()
            .flat_map(|c| c.preds.iter().chain(&c.types))
            .all(|e| e.user_meta == 0x01 && e.discard)
    );
}

#[test]
fn a_name_without_a_namespace_is_refused() {
    // Dgraph's SchemaKey and TypeKey panic on an attribute without a namespace separator.
    let update = pb::SchemaUpdate {
        predicate: "name".into(),
        ..Default::default()
    };
    let e = schema_entry(&update, 1).unwrap_err();
    assert!(matches!(e.0, SchemaErrorEnum::Key(_)), "{e:?}");
    let typ = pb::TypeUpdate {
        type_name: "Person".into(),
        ..Default::default()
    };
    let e = type_entry(&typ, 1).unwrap_err();
    assert!(matches!(e.0, SchemaErrorEnum::Key(_)), "{e:?}");
}
