/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The events the state logs where Dgraph's `schema/schema.go` logs: the same level and the same facts, as fields.

use tracing::Level;
use ultragraphdb_core_catalog::{
    SchemaMode, SchemaState, parse_with_namespace, schema_entry, type_entry,
};
use ultragraphdb_error_store::go_hex_dump;
use ultragraphdb_proto_pb::pb;
use ultragraphdb_store_sim::{CapturedEvent, FaultConfig, LogCapture, Seed, SimClock, SimStore};
use ultragraphdb_traits_store::{Entry, PersistMode, Store, WriteBatch};

/// A schema key whose attribute length runs past its end: Dgraph's `x.Parse` refuses it.
const UNPARSEABLE_KEY: [u8; 12] = [0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 9, b'a'];

fn new_store() -> SimStore {
    SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new())
}

fn write(store: &SimStore, entries: Vec<Entry>) {
    let mut batch = WriteBatch::new();
    entries.into_iter().for_each(|e| batch.push(e));
    store
        .apply(batch, PersistMode::Buffer)
        .expect("the batch is written");
}

/// The store holding `text` at version 5.
fn store_with(text: &str) -> SimStore {
    let store = new_store();
    let (preds, types) = parse_with_namespace(text, 0)
        .expect("the schema parses")
        .into_parts();
    let entries = preds
        .iter()
        .map(|p| schema_entry(p, 5).expect("a namespaced predicate"))
        .chain(
            types
                .iter()
                .map(|t| type_entry(t, 5).expect("a namespaced type")),
        )
        .collect();
    write(&store, entries);
    store
}

/// The events `f` logs.
fn logged(f: impl FnOnce()) -> Vec<CapturedEvent> {
    let capture = LogCapture::new();
    capture.capture(f);
    capture.events()
}

fn with_message<'a>(events: &'a [CapturedEvent], message: &str) -> Vec<&'a CapturedEvent> {
    events
        .iter()
        .filter(|e| e.message() == Some(message))
        .collect()
}

#[test]
fn delete_logs_the_predicate_at_info() {
    let store = store_with("name: string .");
    let state = SchemaState::new();
    let events = logged(|| state.delete(&store, "0-name", 9).expect("deleted"));
    let deleted = with_message(&events, "Deleting schema for predicate");
    assert_eq!(deleted.len(), 1, "{events:?}");
    assert_eq!(deleted[0].level(), Level::INFO);
    assert_eq!(deleted[0].field("attr"), Some("0-name"));
}

#[test]
fn delete_type_logs_the_type_at_info() {
    let store = store_with("type Person {\n name\n}");
    let state = SchemaState::new();
    let events = logged(|| state.delete_type(&store, "0-Person", 9).expect("deleted"));
    let deleted = with_message(&events, "Deleting type definition for type");
    assert_eq!(deleted.len(), 1, "{events:?}");
    assert_eq!(deleted[0].level(), Level::INFO);
    assert_eq!(deleted[0].field("type_name"), Some("0-Person"));
}

#[test]
fn load_from_db_logs_an_unparseable_key_at_error_and_skips_it() {
    let store = store_with("name: string .");
    write(
        &store,
        vec![Entry::put(UNPARSEABLE_KEY.to_vec(), 6, 0x01, vec![1]).with_discard()],
    );
    let state = SchemaState::new();
    let events = logged(|| state.load_from_db(&store).expect("loaded"));
    let refused = with_message(&events, "Error while parsing key");
    assert_eq!(refused.len(), 1, "{events:?}");
    assert_eq!(refused[0].level(), Level::ERROR);
    assert_eq!(
        refused[0].field("key"),
        Some(go_hex_dump(&UNPARSEABLE_KEY).as_str())
    );
    assert!(
        refused[0].field("error").is_some_and(|e| !e.is_empty()),
        "{:?}",
        refused[0]
    );
    assert_eq!(
        state.predicates(),
        vec!["0-name".to_string()],
        "the key is skipped"
    );
}

#[test]
fn loads_log_each_definition_set_at_debug() {
    let store = store_with("name: [string] @index(exact, term) @count .\ntype Person {\n name\n}");
    let state = SchemaState::new();
    let events = logged(|| {
        state.load_from_db(&store).expect("loaded");
        state.load(&store, "0-name").expect("loaded");
    });
    let schemas = with_message(&events, "Setting schema for attr");
    assert_eq!(
        schemas.len(),
        2,
        "one by load_from_db, one by load: {events:?}"
    );
    for set in &schemas {
        assert_eq!(set.level(), Level::DEBUG);
        assert_eq!(set.field("attr"), Some("0-name"));
        assert_eq!(set.field("value_type"), Some("[string]"));
        assert_eq!(set.field("tokenizer"), Some(r#"["exact", "term"]"#));
        assert_eq!(set.field("directive"), Some("INDEX"));
        assert_eq!(set.field("count"), Some("true"));
    }
    let types = with_message(&events, "Setting type definition for type");
    assert_eq!(types.len(), 1, "{events:?}");
    assert_eq!(types[0].level(), Level::DEBUG);
    assert_eq!(types[0].field("type_name"), Some("0-Person"));
    assert!(
        types[0].field("typ").is_some_and(|t| t.contains("0-name")),
        "{:?}",
        types[0]
    );
}

#[test]
fn a_scalar_schema_logs_its_type_without_brackets() {
    let state = SchemaState::new();
    let update = pb::SchemaUpdate {
        predicate: "0-age".into(),
        value_type: pb::posting::ValType::Int as i32,
        ..Default::default()
    };
    let events = logged(|| state.set("0-age", update));
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].field("value_type"), Some("int"));
    assert_eq!(events[0].field("directive"), Some("NONE"));
    assert_eq!(events[0].field("count"), Some("false"));
}

#[test]
fn reads_of_a_missing_schema_log_at_error() {
    let state = SchemaState::new();
    let events = logged(|| {
        assert!(
            state
                .tokenizer(SchemaMode::Query, "0-gone")
                .expect("none")
                .is_empty()
        );
    });
    let missing = with_message(&events, "Schema state not found");
    assert_eq!(missing.len(), 1, "{events:?}");
    assert_eq!(missing[0].level(), Level::ERROR);
    assert_eq!(missing[0].field("pred"), Some("0-gone"));

    let events = logged(|| {
        assert!(
            state
                .factory_create_spec(SchemaMode::Query, "0-gone")
                .is_err()
        );
    });
    assert_eq!(
        with_message(&events, "Schema state not found").len(),
        1,
        "{events:?}"
    );

    // Dgraph's VectorIndexes logs FactoryCreateSpec's error after FactoryCreateSpec logs the missing schema.
    let events = logged(|| {
        assert!(state.vector_indexes(SchemaMode::Query, "0-gone").is_empty());
    });
    let messages: Vec<&str> = events.iter().filter_map(CapturedEvent::message).collect();
    assert_eq!(
        messages,
        vec!["Schema state not found", "Vector Tokenizers error"]
    );
    assert_eq!(events[1].level(), Level::ERROR);
    assert_eq!(events[1].field("pred"), Some("0-gone"));
    assert_eq!(
        events[1].field("error"),
        Some("Schema state not found for 0-gone.")
    );
}
