/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The catalog's scenarios: declarations refused, the type hierarchy's preorder numbers, the catalog surviving a
//! restart, and a catalog entry that does not read back refusing the load.

use cucumber::{World, then, when};
use ultragraphdb_core_catalog::{Catalog, CatalogError, CatalogErrorEnum, PREDICATE};
use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_mapping::{
    Cardinality, PredicateDeclaration, PredicateKind, RelationDeclaration, TypeDeclaration,
};
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_kv::{Durability, FjallStore};
use ultragraphdb_store_sim::{FaultConfig, Seed, SimClock, SimStore};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_traits_store::{Entry, PersistMode, Store, WriteBatch};

/// The ID of sequence `n` in bucket 0, of the built-in `Resource` type.
fn id(n: u64) -> TestResult<SystemId> {
    Ok(SystemId::new(
        TypePrefix::RESOURCE,
        Bucket::new(0)?,
        Sequence::new(n)?,
    ))
}

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
fn declare_age_int(w: &mut Steps) -> TestResult {
    let declaration = PredicateDeclaration::new(ex("age"))
        .with_datatype(xsd("string"))
        .with_tokenizers(vec!["int".to_string()]);
    w.refusals
        .extend(w.catalog.declare_predicate(id(5)?, declaration).err());
    Ok(())
}

#[then("the declaration fails with an error naming age, int and xsd:string")]
fn age_refused(w: &mut Steps) -> TestResult {
    let error = w.refusals.first().context("refused")?.to_string();
    assert!(
        error.contains("age") && error.contains("int") && error.contains("XMLSchema#string"),
        "{error}"
    );
    assert!(
        w.catalog.predicate(id(5)?).is_none(),
        "nothing was declared"
    );
    Ok(())
}

#[when("Employee is declared a subtype of Person and then of Agent")]
fn second_parent(w: &mut Steps) -> TestResult {
    let (employee, person, agent) = (id(1)?, id(2)?, id(3)?);
    for (uid, name) in [(person, "Person"), (agent, "Agent")] {
        w.catalog
            .declare_type(uid, TypeDeclaration::new(ex(name), None, false), None)?;
    }
    w.catalog.declare_type(
        employee,
        TypeDeclaration::new(ex("Employee"), Some(ex("Person")), false),
        Some(person),
    )?;
    w.refusals.extend(
        w.catalog
            .declare_type(
                employee,
                TypeDeclaration::new(ex("Employee"), Some(ex("Agent")), false),
                Some(agent),
            )
            .err(),
    );
    Ok(())
}

#[then("the second declaration fails with an error naming Employee, Person and Agent")]
fn second_parent_refused(w: &mut Steps) -> TestResult {
    let error = w.refusals.first().context("refused")?.to_string();
    for name in ["Employee", "Person", "Agent"] {
        assert!(error.contains(name), "{error}");
    }
    assert_eq!(
        w.catalog.entity_type(id(1)?).and_then(|t| t.parent()),
        Some(&ex("Person"))
    );
    Ok(())
}

#[when("Agent has the subtypes Person and Org, and Person the subtype Employee")]
fn hierarchy(w: &mut Steps) -> TestResult {
    let declare = |steps: &mut Steps, uid: u64, name: &str, parent: Option<(&str, u64)>| {
        let declaration = TypeDeclaration::new(ex(name), parent.map(|(p, _)| ex(p)), false);
        let parent_id = parent.map(|(_, u)| id(u)).transpose()?;
        steps
            .catalog
            .declare_type(id(uid)?, declaration, parent_id)
            .map_err(TestError::from)
    };
    declare(w, 10, "Agent", None)?;
    declare(w, 20, "Person", Some(("Agent", 10)))?;
    declare(w, 30, "Org", Some(("Agent", 10)))?;
    declare(w, 40, "Employee", Some(("Person", 20)))?;
    Ok(())
}

#[then("the preorder numbers place Employee between Person and Org")]
fn preorder_placed(w: &mut Steps) -> TestResult {
    let n = w.catalog.preorder();
    let at = |uid: u64| -> TestResult<u32> { n.get(&id(uid)?).copied().context("a numbered type") };
    assert!(at(20)? < at(40)? && at(40)? < at(30)?, "{n:?}");
    assert_eq!(at(10)?, 1);
    Ok(())
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
    // A directory left by an earlier run of this process id is cleared; there is none on a first run.
    if let Err(e) = std::fs::remove_dir_all(&dir) {
        assert_eq!(
            e.kind(),
            std::io::ErrorKind::NotFound,
            "{}: {e}",
            dir.display()
        );
    }
    dir
}

fn write_catalog(store: &FjallStore, entries: Vec<Entry>) -> TestResult {
    let mut batch = WriteBatch::new();
    entries.into_iter().for_each(|e| batch.push(e));
    store.apply(batch, PersistMode::Sync)?;
    Ok(())
}

#[when("declarations are made and the store is closed and reopened")]
fn catalog_restart(w: &mut Steps) -> TestResult {
    let dir = fjall_dir();
    let mut catalog = Catalog::initial(3, id(1)?);
    catalog.declare_predicate(
        id(7)?,
        PredicateDeclaration::new(ex("name"))
            .with_kind(PredicateKind::Property)
            .with_cardinality(Cardinality::Single)
            .with_tokenizers(vec!["exact".into(), "term".into()])
            .with_unique(true),
    )?;
    catalog.declare_type(
        id(8)?,
        TypeDeclaration::new(ex("Person"), None, false),
        None,
    )?;
    catalog.declare_relation(
        id(9)?,
        RelationDeclaration::new(ex("Employment"), vec![ex("employer")]),
    );
    let store = FjallStore::open(&dir, Durability::Filesystem)?;
    write_catalog(&store, catalog.entries(2))?;
    drop(store);
    let reopened = FjallStore::open(&dir, Durability::Filesystem)?;
    let loaded = Catalog::load(&reopened.snapshot(u64::MAX)?, 3)?;
    w.catalogs = Some((catalog, loaded));
    drop(reopened);
    std::fs::remove_dir_all(&dir).context("the store's directory is removed")?;
    Ok(())
}

#[then("the loaded catalog equals the catalog before closing")]
fn catalog_equal(w: &mut Steps) -> TestResult {
    let (before, after) = w.catalogs.take().context("the catalogs before and after")?;
    assert_eq!(after, before);
    Ok(())
}

#[when("a predicate's declaration is removed and the store is reopened")]
fn removal_restart(w: &mut Steps) -> TestResult {
    let dir = fjall_dir();
    let mut catalog = Catalog::initial(0, id(1)?);
    catalog.declare_predicate(id(7)?, PredicateDeclaration::new(ex("nick")))?;
    let store = FjallStore::open(&dir, Durability::Filesystem)?;
    write_catalog(&store, catalog.entries(2))?;
    assert!(catalog.remove_predicate(id(7)?), "nick was declared");
    write_catalog(&store, vec![Catalog::removal(0, PREDICATE, id(7)?, 3)])?;
    drop(store);
    let reopened = FjallStore::open(&dir, Durability::Filesystem)?;
    let loaded = Catalog::load(&reopened.snapshot(u64::MAX)?, 0)?;
    w.catalogs = Some((catalog, loaded));
    drop(reopened);
    std::fs::remove_dir_all(&dir).context("the store's directory is removed")?;
    Ok(())
}

#[then("the loaded catalog does not declare the predicate")]
fn removal_kept(w: &mut Steps) -> TestResult {
    let (before, after) = w.catalogs.take().context("the catalogs before and after")?;
    assert!(after.predicate(id(7)?).is_none());
    assert!(after.predicate(id(1)?).is_some(), "rdf:type stays declared");
    assert_eq!(after, before);
    Ok(())
}

#[when("a namespace's catalog is loaded from a store holding a catalog key that does not parse")]
fn load_unparseable_key(w: &mut Steps) -> TestResult {
    let store = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    let mut batch = WriteBatch::new();
    Catalog::initial(0, id(1)?)
        .entries(2)
        .into_iter()
        .for_each(|e| batch.push(e));
    batch.push(Entry::put(UNPARSEABLE_KEY.to_vec(), 3, 0x01, vec![1]).with_discard());
    store
        .apply(batch, PersistMode::Buffer)
        .context("the batch is written")?;
    w.loaded = Some(Catalog::load(
        &store.snapshot(u64::MAX).context("a snapshot")?,
        0,
    ));
    Ok(())
}

#[then("the load fails with a corruption error naming the key, and nothing panics")]
fn load_refused(w: &mut Steps) -> TestResult {
    let Err(error) = w.loaded.take().context("loaded")? else {
        return Err(TestError::new("the load is refused"));
    };
    assert!(
        matches!(
            &error.0,
            CatalogErrorEnum::Store(StoreError(StoreErrorEnum::Corrupted { key, .. }))
                if key.as_slice() == UNPARSEABLE_KEY
        ),
        "{error:?}"
    );
    Ok(())
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
