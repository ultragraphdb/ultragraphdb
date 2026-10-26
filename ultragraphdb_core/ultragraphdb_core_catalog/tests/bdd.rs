/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use cucumber::{World, then, when};
use ultragraphdb_core_catalog::{
    Catalog, CatalogError, PREDICATE, ParsedSchema, SchemaError, SchemaMode, SchemaState,
    complete_initial_schema, complete_initial_types, parse, parse_with_namespace, schema_entry,
    type_entry,
};
use ultragraphdb_core_keys::parse_namespace;
use ultragraphdb_core_mapping::{
    Cardinality, PredicateDeclaration, PredicateKind, RelationDeclaration, TypeDeclaration,
};
use ultragraphdb_core_types::TypeId;
use ultragraphdb_error_store::go_hex_dump;
use ultragraphdb_proto_pb::pb;
use ultragraphdb_store_kv::{Durability, FjallStore};
use ultragraphdb_store_sim::{CapturedEvent, FaultConfig, LogCapture, Seed, SimClock, SimStore};
use ultragraphdb_traits_store::{Entry, PersistMode, Store, WriteBatch};

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
    /// The schema state before the store was closed, and as loaded after it was reopened.
    stored: Option<(SchemaDump, SchemaDump)>,
    /// The events a load logged, and the predicates it loaded.
    logged: Option<(Vec<CapturedEvent>, Vec<String>)>,
    catalog: Catalog,
    refusals: Vec<CatalogError>,
    catalogs: Option<(Catalog, Catalog)>,
}

/// A schema key whose attribute length runs past its end: Dgraph's `x.Parse` refuses it.
const UNPARSEABLE_KEY: [u8; 12] = [0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 9, b'a'];

/// A schema state's predicates and types, sorted by name.
type SchemaDump = (Vec<pb::SchemaUpdate>, Vec<pb::TypeUpdate>);

fn dump(state: &SchemaState) -> SchemaDump {
    let mut preds = state.predicates();
    preds.sort();
    let mut types = state.types();
    types.sort();
    (
        preds
            .iter()
            .filter_map(|p| state.get(SchemaMode::Query, p))
            .collect(),
        types.iter().filter_map(|t| state.get_type(t)).collect(),
    )
}

/// Writes `text`, parsed in namespace `ns`, at `ts` as one batch, and sets it in `state`.
fn apply_schema(store: &SimStore, state: &SchemaState, text: &str, ns: u64, ts: u64) {
    let (preds, types) = parse_with_namespace(text, ns)
        .expect("the schema parses")
        .into_parts();
    let mut batch = WriteBatch::new();
    for p in &preds {
        batch.push(schema_entry(p, ts).expect("a namespaced predicate"));
    }
    for t in &types {
        batch.push(type_entry(t, ts).expect("a namespaced type"));
    }
    batch.set_applied_ts(ts);
    store
        .apply(batch, PersistMode::Buffer)
        .expect("the batch is written");
    preds
        .into_iter()
        .for_each(|p| state.set(&p.predicate.clone(), p));
    types
        .into_iter()
        .for_each(|t| state.set_type(&t.type_name.clone(), t));
}

/// Closes the store, synced, and reopens it, then loads a fresh state from it.
fn reopen_and_load(store: &SimStore) -> SchemaDump {
    store.sync().expect("the store syncs");
    store.crash();
    store.recover();
    let state = SchemaState::new();
    state.load_from_db(store).expect("the schema loads");
    dump(&state)
}

fn new_store() -> SimStore {
    SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new())
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
    assert_eq!(parse_namespace(&p.predicate), Ok(want));
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
        s.state
            .tokenizer_names(SchemaMode::Query, &pred)
            .expect("the tokenizers exist"),
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

#[when("a schema is applied and the store is closed and reopened")]
fn schema_applied_and_reopened(s: &mut Steps) {
    let store = new_store();
    let state = SchemaState::new();
    apply_schema(
        &store,
        &state,
        "name: string @index(exact, term) @upsert @lang .\nfriend: [uid] @reverse @count .\n\
         vec: float32vector @index(hnsw(metric:\"cosine\")) .\ntype Person {\n name\n friend\n}",
        0,
        2,
    );
    apply_schema(
        &store,
        &state,
        "age: int @index(int) .\ntype Animal {\n age\n}",
        5,
        3,
    );
    let before = dump(&state);
    assert_eq!((before.0.len(), before.1.len()), (4, 2));
    s.stored = Some((before, reopen_and_load(&store)));
}

#[then("the loaded schema state equals the state before closing")]
fn loaded_equals_before(s: &mut Steps) {
    let (before, after) = s.stored.as_ref().expect("the store was reopened");
    assert_eq!(after, before);
}

#[when("a predicate's schema is deleted and the store is reopened")]
fn predicate_deleted_and_reopened(s: &mut Steps) {
    let store = new_store();
    let state = SchemaState::new();
    apply_schema(
        &store,
        &state,
        "name: string @index(exact) .\nage: int .",
        0,
        2,
    );
    state
        .delete(&store, "0-name", 3)
        .expect("the schema is deleted");
    let before = dump(&state);
    s.stored = Some((before, reopen_and_load(&store)));
}

#[then("the loaded schema does not hold the predicate")]
fn loaded_lacks_predicate(s: &mut Steps) {
    let (before, after) = s.stored.as_ref().expect("the store was reopened");
    let names: Vec<&str> = after.0.iter().map(|p| p.predicate.as_str()).collect();
    assert_eq!(names, ["0-age"]);
    assert_eq!(after, before);
}

#[when("the schema is loaded from a store holding a schema key that does not parse")]
fn load_unparseable_key(w: &mut Steps) {
    let store = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    apply_schema(&store, &w.state, "name: string .", 0, 5);
    let mut batch = WriteBatch::new();
    batch.push(Entry::put(UNPARSEABLE_KEY.to_vec(), 6, 0x01, vec![1]).with_discard());
    store
        .apply(batch, PersistMode::Buffer)
        .expect("the batch is written");
    let capture = LogCapture::new();
    capture.capture(|| {
        w.state
            .load_from_db(&store)
            .expect("the load skips the key");
    });
    w.logged = Some((capture.events(), w.state.predicates()));
}

#[then(
    "the load skips the key and emits an error event `Error while parsing key` with the key as a field"
)]
fn unparseable_key_logged(w: &mut Steps) {
    let (events, predicates) = w.logged.take().expect("loaded");
    assert_eq!(predicates, vec!["0-name".to_string()], "the key is skipped");
    let refused: Vec<&CapturedEvent> = events
        .iter()
        .filter(|e| e.message() == Some("Error while parsing key"))
        .collect();
    assert_eq!(refused.len(), 1, "{events:?}");
    assert_eq!(refused[0].level(), tracing::Level::ERROR);
    assert_eq!(
        refused[0].field("key"),
        Some(go_hex_dump(&UNPARSEABLE_KEY).as_str())
    );
}

fn xsd(local: &str) -> oxrdf::NamedNode {
    oxrdf::NamedNode::new_unchecked(format!("http://www.w3.org/2001/XMLSchema#{local}"))
}

fn ex(name: &str) -> oxrdf::NamedNode {
    oxrdf::NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

#[when("age is declared with datatype xsd:string and an int index")]
fn declare_age_int(w: &mut Steps) {
    let declaration = PredicateDeclaration::new(ex("age"))
        .with_datatype(xsd("string"))
        .with_tokenizers(vec!["int".to_string()]);
    w.refusals
        .extend(w.catalog.declare_predicate(5, declaration).err());
}

#[then("the declaration fails with an error naming age, int and xsd:string")]
fn age_refused(w: &mut Steps) {
    let error = w.refusals.first().expect("refused").to_string();
    assert!(
        error.contains("age") && error.contains("int") && error.contains("XMLSchema#string"),
        "{error}"
    );
    assert!(w.catalog.predicate(5).is_none(), "nothing was declared");
}

#[when("Employee is declared a subtype of Person and then of Agent")]
fn second_parent(w: &mut Steps) {
    let (employee, person, agent) = (1, 2, 3);
    for (uid, name) in [(person, "Person"), (agent, "Agent")] {
        w.catalog
            .declare_type(uid, TypeDeclaration::new(ex(name), None, false), None)
            .unwrap();
    }
    w.catalog
        .declare_type(
            employee,
            TypeDeclaration::new(ex("Employee"), Some(ex("Person")), false),
            Some(person),
        )
        .unwrap();
    w.refusals.extend(
        w.catalog
            .declare_type(
                employee,
                TypeDeclaration::new(ex("Employee"), Some(ex("Agent")), false),
                Some(agent),
            )
            .err(),
    );
}

#[then("the second declaration fails with an error naming Employee, Person and Agent")]
fn second_parent_refused(w: &mut Steps) {
    let error = w.refusals.first().expect("refused").to_string();
    for name in ["Employee", "Person", "Agent"] {
        assert!(error.contains(name), "{error}");
    }
    assert_eq!(
        w.catalog.entity_type(1).and_then(|t| t.parent()),
        Some(&ex("Person"))
    );
}

#[when("Agent has the subtypes Person and Org, and Person the subtype Employee")]
fn hierarchy(w: &mut Steps) {
    let declare = |w: &mut Steps, uid: u64, name: &str, parent: Option<(&str, u64)>| {
        let declaration = TypeDeclaration::new(ex(name), parent.map(|(p, _)| ex(p)), false);
        w.catalog
            .declare_type(uid, declaration, parent.map(|(_, u)| u))
            .unwrap();
    };
    declare(w, 10, "Agent", None);
    declare(w, 20, "Person", Some(("Agent", 10)));
    declare(w, 30, "Org", Some(("Agent", 10)));
    declare(w, 40, "Employee", Some(("Person", 20)));
}

#[then("the preorder numbers place Employee between Person and Org")]
fn preorder_placed(w: &mut Steps) {
    let n = w.catalog.preorder();
    assert!(n[&20] < n[&40] && n[&40] < n[&30], "{n:?}");
    assert_eq!(n[&10], 1);
}

/// A fjall store in a fresh directory of its own.
fn fjall_dir() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "catalog-bdd-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn write_catalog(store: &FjallStore, entries: Vec<Entry>) {
    let mut batch = WriteBatch::new();
    entries.into_iter().for_each(|e| batch.push(e));
    store.apply(batch, PersistMode::Sync).unwrap();
}

#[when("declarations are made and the store is closed and reopened")]
fn catalog_restart(w: &mut Steps) {
    let dir = fjall_dir();
    let mut catalog = Catalog::initial(3, 1);
    catalog
        .declare_predicate(
            7,
            PredicateDeclaration::new(ex("name"))
                .with_kind(PredicateKind::Property)
                .with_cardinality(Cardinality::Single)
                .with_tokenizers(vec!["exact".into(), "term".into()])
                .with_unique(true),
        )
        .unwrap();
    catalog
        .declare_type(8, TypeDeclaration::new(ex("Person"), None, false), None)
        .unwrap();
    catalog.declare_relation(
        9,
        RelationDeclaration::new(ex("Employment"), vec![ex("employer")]),
    );
    let store = FjallStore::open(&dir, Durability::Filesystem).unwrap();
    write_catalog(&store, catalog.entries(2));
    drop(store);
    let store = FjallStore::open(&dir, Durability::Filesystem).unwrap();
    let loaded = Catalog::load(&store.snapshot(u64::MAX).unwrap(), 3).unwrap();
    w.catalogs = Some((catalog, loaded));
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}

#[then("the loaded catalog equals the catalog before closing")]
fn catalog_equal(w: &mut Steps) {
    let (before, after) = w.catalogs.take().unwrap();
    assert_eq!(after, before);
}

#[when("a predicate's declaration is removed and the store is reopened")]
fn removal_restart(w: &mut Steps) {
    let dir = fjall_dir();
    let mut catalog = Catalog::initial(0, 1);
    catalog
        .declare_predicate(7, PredicateDeclaration::new(ex("nick")))
        .unwrap();
    let store = FjallStore::open(&dir, Durability::Filesystem).unwrap();
    write_catalog(&store, catalog.entries(2));
    catalog.remove_predicate(7);
    write_catalog(&store, vec![Catalog::removal(0, PREDICATE, 7, 3)]);
    drop(store);
    let store = FjallStore::open(&dir, Durability::Filesystem).unwrap();
    let loaded = Catalog::load(&store.snapshot(u64::MAX).unwrap(), 0).unwrap();
    w.catalogs = Some((catalog, loaded));
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}

#[then("the loaded catalog does not declare the predicate")]
fn removal_kept(w: &mut Steps) {
    let (before, after) = w.catalogs.take().unwrap();
    assert!(after.predicate(7).is_none());
    assert!(after.predicate(1).is_some(), "rdf:type stays declared");
    assert_eq!(after, before);
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_test_bdd::features_dir()),
    );
}
