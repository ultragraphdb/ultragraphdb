/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Loading and deleting through the `Store` trait: Dgraph's `Load`, `LoadFromDb`, `Delete` and `DeleteType` run
//! over the same writes as Dgraph's, against what Dgraph loads from Badger; the store's faults; and the refusals
//! where Dgraph panics.
//!
//! [`loads_match_dgraph`] takes any `Store`: it runs here on the simulated store, and runs on the fjall store by
//! adding a call with it.

use serde::Deserialize;
use ultragraphdb_core_keys::{schema_key, type_key};
use ultragraphdb_core_schema::{
    SchemaErrorEnum, SchemaMode, SchemaState, parse_with_namespace, schema_entry, type_entry,
};
use ultragraphdb_error_store::StoreErrorEnum;
use ultragraphdb_proto_pb::pb;
use ultragraphdb_store_kv::{Durability, FjallStore};
use ultragraphdb_store_sim::{Corruption, Fault, FaultConfig, Seed, SimClock, SimStore};
use ultragraphdb_test_bdd::Golden;
use ultragraphdb_traits_store::{Entry, PersistMode, Store, WriteBatch};

use crate::golden_json::{TypeJson, UpdateJson, type_json, update_json};

#[derive(Debug, Deserialize)]
struct OpJson {
    op: String,
    text: String,
    name: String,
    ns: u64,
    ts: u64,
}

#[derive(Debug, Deserialize)]
struct LoadJson {
    predicate: String,
    err: String,
    found: bool,
    schema: Option<UpdateJson>,
}

#[derive(Debug, Deserialize)]
struct LoadCase {
    name: String,
    ops: Vec<OpJson>,
    preds: Vec<UpdateJson>,
    types: Vec<TypeJson>,
    loads: Vec<LoadJson>,
}

/// The meta byte of a schema or type entry, Dgraph's `BitSchemaPosting`.
const SCHEMA_META: u8 = 0x01;

/// The key `tools/golden` writes under the schema prefix: a namespace and one byte, too short for an attribute.
const MALFORMED_KEY: [u8; 10] = [0x01, 0, 0, 0, 0, 0, 0, 0, 0, 7];

/// The cases where Dgraph's `LoadFromDb` loads a deleted definition, and the name it loads. Badger's stream
/// iterates all versions, so the newest version of a deleted key is its deletion, whose empty value `loadFromDB`
/// reads as a default schema `{Predicate: attr, ValueType: DEFAULT}` or an empty type `{TypeName: attr}`; the
/// port's scan omits a key whose newest version is a deletion, as `Load` does in Dgraph. Deviation.
const RESURRECTED: [(&str, &str); 2] = [
    ("a deleted predicate", "0-name"),
    ("a deleted type", "0-Person"),
];

fn new_store() -> SimStore {
    SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new())
}

fn golden() -> Golden<LoadCase> {
    let path = ultragraphdb_test_bdd::testdata_dir().join("golden/schema_storage_loads.json");
    ultragraphdb_test_bdd::load_golden(&path)
}

/// Writes `text`, parsed in namespace `ns`, at `ts` as one batch.
fn write_schema<S: Store>(store: &S, text: &str, ns: u64, ts: u64) {
    let (preds, types) = parse_with_namespace(text, ns)
        .expect("the schema parses")
        .into_parts();
    let mut batch = WriteBatch::new();
    preds
        .iter()
        .for_each(|p| batch.push(schema_entry(p, ts).expect("a namespaced predicate")));
    types
        .iter()
        .for_each(|t| batch.push(type_entry(t, ts).expect("a namespaced type")));
    store
        .apply(batch, PersistMode::Buffer)
        .expect("the batch is written");
}

fn write_one<S: Store>(store: &S, entry: Entry) {
    let mut batch = WriteBatch::new();
    batch.push(entry);
    store
        .apply(batch, PersistMode::Buffer)
        .expect("the batch is written");
}

/// Applies an operation of `tools/golden`'s load cases, as Dgraph's generator did.
fn apply_op<S: Store>(store: &S, state: &SchemaState, op: &OpJson) {
    let empty = |key: Vec<u8>| Entry::put(key, op.ts, SCHEMA_META, Vec::new()).with_discard();
    match op.op.as_str() {
        "schema" => write_schema(store, &op.text, op.ns, op.ts),
        "delete" => state.delete(store, &op.name, op.ts).expect("deleted"),
        "delete_type" => state.delete_type(store, &op.name, op.ts).expect("deleted"),
        "empty_schema" => write_one(store, empty(schema_key(&op.name).expect("a name"))),
        "empty_type" => write_one(store, empty(type_key(&op.name).expect("a name"))),
        "ban" => store.ban_namespace(op.ns).expect("banned"),
        "malformed" => write_one(
            store,
            Entry::put(MALFORMED_KEY.to_vec(), op.ts, SCHEMA_META, vec![1]).with_discard(),
        ),
        other => panic!("unknown op {other}"),
    }
}

/// The predicates and types of `state`, sorted by name, as the generator writes them.
fn loaded(state: &SchemaState) -> (Vec<UpdateJson>, Vec<TypeJson>) {
    let mut preds = state.predicates();
    preds.sort();
    let mut types = state.types();
    types.sort();
    (
        preds
            .iter()
            .filter_map(|p| state.get(SchemaMode::Query, p))
            .map(|p| update_json(&p))
            .collect(),
        types
            .iter()
            .filter_map(|t| state.get_type(t))
            .map(|t| type_json(&t))
            .collect(),
    )
}

/// What the port loads where Dgraph loads `want`: the same, less the definition Dgraph loads back from its
/// deletion, which the golden file is checked to hold.
fn expected(c: &LoadCase) -> (Vec<UpdateJson>, Vec<TypeJson>) {
    let (mut preds, mut types) = (c.preds.clone(), c.types.clone());
    if let Some((_, name)) = RESURRECTED.iter().find(|(case, _)| *case == c.name) {
        let before = (preds.len(), types.len());
        preds.retain(|p| !(p.predicate == *name && p.value_type == 0 && p.tokenizer.is_empty()));
        types.retain(|t| !(t.type_name == *name && t.fields.is_empty()));
        assert_eq!(
            before.0 + before.1,
            preds.len() + types.len() + 1,
            "{}: Dgraph loads {name} back from its deletion",
            c.name
        );
    }
    (preds, types)
}

/// Runs every load case of the golden file on stores `open` returns, one per case, and compares what
/// `load_from_db` and `load` give with what Dgraph's `LoadFromDb` and `Load` gave.
fn loads_match_dgraph<S: Store>(open: impl Fn() -> S) {
    let golden = golden();
    assert_eq!(golden.cases().len(), 10);
    for c in golden.cases() {
        let store = open();
        let state = SchemaState::new();
        c.ops.iter().for_each(|op| apply_op(&store, &state, op));

        // Dgraph's LoadFromDb resets the state first; a definition only in memory is gone after it.
        state.set(
            "0-memory",
            pb::SchemaUpdate {
                predicate: "0-memory".into(),
                ..Default::default()
            },
        );
        state.load_from_db(&store).expect("the schema loads");
        assert_eq!(loaded(&state), expected(c), "{}", c.name);

        for l in &c.loads {
            let state = SchemaState::new();
            let result = state.load(&store, &l.predicate);
            let err = result.err().map(|e| e.to_string()).unwrap_or_default();
            assert_eq!(err, l.err, "{}: load {:?}", c.name, l.predicate);
            let got = state.get(SchemaMode::Query, &l.predicate);
            assert_eq!(got.is_some(), l.found, "{}: load {:?}", c.name, l.predicate);
            assert_eq!(
                got.as_ref().map(update_json),
                l.schema,
                "{}: load {:?}",
                c.name,
                l.predicate
            );
        }
    }
}

#[test]
fn loads_match_dgraph_on_the_simulated_store() {
    loads_match_dgraph(new_store);
}

/// A directory of its own under Bazel's `TEST_TMPDIR`, or the system's temporary directory under Cargo.
fn fjall_dir(name: &str) -> std::path::PathBuf {
    let root =
        std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, std::path::PathBuf::from);
    let dir = root.join(format!("schema-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn loads_match_dgraph_on_the_fjall_store() {
    let root = fjall_dir("loads");
    let next = std::cell::Cell::new(0u32);
    loads_match_dgraph(|| {
        next.set(next.get() + 1);
        FjallStore::open(root.join(next.get().to_string()), Durability::Process)
            .expect("the store opens")
    });
    assert_eq!(next.get(), 10, "one fjall store per case");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_schema_write_and_a_deletion_survive_a_restart_of_the_fjall_store() {
    let dir = fjall_dir("restart");
    let open = || FjallStore::open(&dir, Durability::Process).expect("the store opens");
    let store = open();
    write_schema(
        &store,
        "name: string @index(exact) .\nage: int .\ntype Person {\n name\n}",
        0,
        2,
    );
    SchemaState::new()
        .delete(&store, "0-age", 3)
        .expect("deleted");
    drop(store);

    let store = open();
    let state = SchemaState::new();
    state
        .load_from_db(&store)
        .expect("the schema loads after the restart");
    let name = state
        .get(SchemaMode::Query, "0-name")
        .expect("the write survived");
    assert_eq!(name.tokenizer, vec!["exact".to_string()]);
    assert_eq!(
        state.get(SchemaMode::Query, "0-age"),
        None,
        "the deletion survived"
    );
    assert!(state.get_type("0-Person").is_some());

    state.delete_type(&store, "0-Person", 4).expect("deleted");
    drop(store);
    let store = open();
    let state = SchemaState::new();
    state
        .load_from_db(&store)
        .expect("the schema loads after the second restart");
    assert_eq!(state.get_type("0-Person"), None);
    assert!(state.get(SchemaMode::Query, "0-name").is_some());
    let one = SchemaState::new();
    one.load(&store, "0-age").expect("loads");
    assert_eq!(one.get(SchemaMode::Query, "0-age"), None);
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_empty_predicate_is_refused() {
    let e = SchemaState::new().load(&new_store(), "").unwrap_err();
    assert!(matches!(e.0, SchemaErrorEnum::State(_)), "{e:?}");
    assert_eq!(e.to_string(), "Empty predicate");
}

#[test]
fn a_predicate_without_a_namespace_is_refused() {
    // Dgraph's SchemaKey panics on it: the name has no namespace separator. Deviation.
    let e = SchemaState::new().load(&new_store(), "name").unwrap_err();
    assert!(matches!(e.0, SchemaErrorEnum::Key(_)), "{e:?}");
}

#[test]
fn load_drops_the_background_schema_and_keeps_an_absent_one() {
    let store = new_store();
    let state = SchemaState::new();
    let held = pb::SchemaUpdate {
        predicate: "0-name".into(),
        value_type: 9,
        ..Default::default()
    };
    state.set("0-name", held.clone());
    state.set_mut_schema("0-name", held.clone());
    state.load(&store, "0-name").expect("loads");
    // Nothing is stored: Dgraph's Load returns without touching the predicate, but drops its background schema.
    assert!(!state.indexing_in_progress());
    assert_eq!(state.get(SchemaMode::Write, "0-name"), Some(held));
}

#[test]
fn load_reads_the_newest_version() {
    let store = new_store();
    write_schema(&store, "name: string .", 0, 2);
    write_schema(&store, "name: int @index(int) .", 0, 4);
    let state = SchemaState::new();
    state.load(&store, "0-name").expect("loads");
    let got = state.get(SchemaMode::Query, "0-name").expect("loaded");
    assert_eq!(
        (got.value_type, got.tokenizer),
        (2, vec!["int".to_string()])
    );
}

#[test]
fn delete_writes_a_deletion_at_its_timestamp() {
    let store = new_store();
    let state = SchemaState::new();
    write_schema(&store, "name: string .\ntype Person {\n name\n}", 0, 2);
    state.load_from_db(&store).expect("loads");
    state.set_mut_schema("0-name", pb::SchemaUpdate::default());

    state.delete(&store, "0-name", 3).expect("deleted");
    state.delete_type(&store, "0-Person", 4).expect("deleted");

    let contents = store.contents();
    for (key, ts) in [
        (schema_key("0-name").expect("key"), 3),
        (type_key("0-Person").expect("key"), 4),
    ] {
        let versions = contents.versions(&key);
        assert_eq!(versions.len(), 2);
        assert!(versions[0].is_deletion());
        assert_eq!(versions[0].version(), ts);
        assert_eq!(
            (
                versions[1].version(),
                versions[1].meta(),
                versions[1].discard()
            ),
            (2, SCHEMA_META, true)
        );
    }
    assert!(state.predicates().is_empty() && state.types().is_empty());
    assert!(!state.indexing_in_progress());
    // A deletion records no applied timestamp: it is no commit.
    assert_eq!(store.applied_ts(), 0);
}

#[test]
fn a_failed_delete_keeps_the_definition() {
    let store = new_store();
    let state = SchemaState::new();
    write_schema(&store, "name: string .\ntype Person {\n name\n}", 0, 2);
    state.load_from_db(&store).expect("loads");

    store.inject(Fault::FailNextBatch);
    let e = state.delete(&store, "0-name", 3).unwrap_err();
    assert!(
        matches!(&e.0, SchemaErrorEnum::Store(s) if matches!(s.0, StoreErrorEnum::WriteFailed(_))),
        "{e:?}"
    );
    let e = state.delete_type(&store, "0-Person", 3).unwrap_err();
    assert!(
        matches!(&e.0, SchemaErrorEnum::Store(s) if s.0 == StoreErrorEnum::Poisoned),
        "{e:?}"
    );
    assert_eq!(
        e.to_string(),
        "The store is poisoned by an earlier failed write and must be reopened"
    );
    assert!(state.get(SchemaMode::Query, "0-name").is_some());
    assert!(state.get_type("0-Person").is_some());
}

#[test]
fn a_delete_of_a_name_without_a_namespace_is_refused() {
    // Dgraph's SchemaKey and TypeKey panic on it. Deviation.
    let store = new_store();
    let state = SchemaState::new();
    let e = state.delete(&store, "name", 3).unwrap_err();
    assert!(matches!(e.0, SchemaErrorEnum::Key(_)), "{e:?}");
    let e = state.delete_type(&store, "Person", 3).unwrap_err();
    assert!(matches!(e.0, SchemaErrorEnum::Key(_)), "{e:?}");
    assert!(store.contents().keys().is_empty());
}

#[test]
fn a_failed_read_fails_the_load() {
    let store = new_store();
    write_schema(&store, "name: string .\nage: int .\nfriend: [uid] .", 0, 2);
    let state = SchemaState::new();

    store.inject(Fault::FailNextRead);
    let e = state.load(&store, "0-name").unwrap_err();
    assert!(
        matches!(&e.0, SchemaErrorEnum::Store(s) if matches!(s.0, StoreErrorEnum::ReadFailed { .. })),
        "{e:?}"
    );

    // Dgraph's stream logs a key whose value it cannot read and loads without it; the port fails the load rather
    // than start without a predicate. Deviation.
    store.inject(Fault::FailNextScanAfter(1));
    let e = state.load_from_db(&store).unwrap_err();
    assert!(
        matches!(&e.0, SchemaErrorEnum::Store(s) if matches!(s.0, StoreErrorEnum::ReadFailed { .. })),
        "{e:?}"
    );
    state
        .load_from_db(&store)
        .expect("loads once the read succeeds");
    assert_eq!(state.predicates().len(), 3);
}

#[test]
fn a_truncated_value_fails_the_load() {
    let store = new_store();
    write_schema(&store, "name: string .", 0, 2);
    let key = schema_key("0-name").expect("key");
    store.corrupt(&key, 2, Corruption::Truncate);
    let state = SchemaState::new();
    for e in [
        state.load(&store, "0-name").unwrap_err(),
        state.load_from_db(&store).unwrap_err(),
    ] {
        assert!(
            matches!(&e.0, SchemaErrorEnum::Store(s) if matches!(s.0, StoreErrorEnum::Corrupted { .. })),
            "{e:?}"
        );
    }
}

#[test]
fn an_undecodable_value_fails_the_load() {
    // The flipped byte lands in the name, which is then not UTF-8: the decoder refuses it. Dgraph's Load and
    // loadFromDB exit the process on an unmarshal error. Deviation.
    let store = new_store();
    write_schema(&store, "name: string .\ntype Person {\n name\n}", 0, 2);
    let state = SchemaState::new();

    let key = schema_key("0-name").expect("key");
    store.corrupt(&key, 2, Corruption::FlipValueByte);
    let e = state.load(&store, "0-name").unwrap_err();
    assert!(matches!(e.0, SchemaErrorEnum::Decode(_)), "{e:?}");
    assert!(
        e.to_string()
            .starts_with("failed to decode Protobuf message"),
        "{e}"
    );
    let e = state.load_from_db(&store).unwrap_err();
    assert!(matches!(e.0, SchemaErrorEnum::Decode(_)), "{e:?}");
    assert!(
        e.to_string()
            .starts_with("Error while loading schema from db: failed to decode Protobuf message"),
        "{e}"
    );

    write_schema(&store, "name: string .", 0, 3);
    let key = type_key("0-Person").expect("key");
    store.corrupt(&key, 2, Corruption::FlipValueByte);
    let e = state.load_from_db(&store).unwrap_err();
    assert!(
        e.to_string()
            .starts_with("Error while loading types from db: failed to decode Protobuf message"),
        "{e}"
    );
}

#[test]
fn a_store_down_refuses_the_load() {
    let store = new_store();
    store.crash();
    let state = SchemaState::new();
    let e = state.load_from_db(&store).unwrap_err();
    assert!(
        matches!(&e.0, SchemaErrorEnum::Store(s) if s.0 == StoreErrorEnum::Poisoned),
        "{e:?}"
    );
}
