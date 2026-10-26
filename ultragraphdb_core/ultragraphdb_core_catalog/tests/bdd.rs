/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The catalog's scenarios: declarations refused, the type hierarchy's preorder numbers, the catalog surviving a
//! restart, and a catalog entry that does not read back refusing the load.

use cucumber::{World, then, when};
use ultragraphdb_core_catalog::{Catalog, CatalogError, CatalogErrorEnum, PREDICATE};
use ultragraphdb_core_mapping::{
    Cardinality, PredicateDeclaration, PredicateKind, RelationDeclaration, TypeDeclaration,
};
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_kv::{Durability, FjallStore};
use ultragraphdb_store_sim::{FaultConfig, Seed, SimClock, SimStore};
use ultragraphdb_traits_store::{Entry, PersistMode, Store, WriteBatch};

#[derive(Debug, Default, World)]
struct Steps {
    catalog: Catalog,
    refusals: Vec<CatalogError>,
    catalogs: Option<(Catalog, Catalog)>,
    loaded: Option<Result<Catalog, CatalogError>>,
}

/// A key under namespace 0's catalog prefix that ends before its UID: no catalog key parses from it.
const UNPARSEABLE_KEY: [u8; 10] = [0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0x01];

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

#[when("a namespace's catalog is loaded from a store holding a catalog key that does not parse")]
fn load_unparseable_key(w: &mut Steps) {
    let store = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    let mut batch = WriteBatch::new();
    Catalog::initial(0, 1)
        .entries(2)
        .into_iter()
        .for_each(|e| batch.push(e));
    batch.push(Entry::put(UNPARSEABLE_KEY.to_vec(), 3, 0x01, vec![1]).with_discard());
    store
        .apply(batch, PersistMode::Buffer)
        .expect("the batch is written");
    w.loaded = Some(Catalog::load(
        &store.snapshot(u64::MAX).expect("a snapshot"),
        0,
    ));
}

#[then("the load fails with a corruption error naming the key, and nothing panics")]
fn load_refused(w: &mut Steps) {
    let error = w
        .loaded
        .take()
        .expect("loaded")
        .expect_err("the load is refused");
    assert!(
        matches!(
            &error.0,
            CatalogErrorEnum::Store(StoreError(StoreErrorEnum::Corrupted { key, .. }))
                if key.as_slice() == UNPARSEABLE_KEY
        ),
        "{error:?}"
    );
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_test_bdd::features_dir()),
    );
}
