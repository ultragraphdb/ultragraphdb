/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::NamedNode;
use ultragraphdb_core_catalog::{
    Catalog, CatalogErrorEnum, ENTITY_TYPE, PREDICATE, RELATION, encode_declaration,
};
use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_mapping::{
    Declaration, PredicateDeclaration, PredicateKind, RelationDeclaration, TypeDeclaration,
};
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_sim::{FaultConfig, Seed, SimClock, SimStore};
use ultragraphdb_tool_bdd::{TestError, TestResult};
use ultragraphdb_traits_store::{Entry, PersistMode, Store, WriteBatch};

/// The ID of sequence `n` in bucket 0, of the built-in `Resource` type.
fn id(n: u64) -> TestResult<SystemId> {
    Ok(SystemId::new(
        TypePrefix::RESOURCE,
        Bucket::new(0)?,
        Sequence::new(n)?,
    ))
}

fn write(store: &SimStore, entries: Vec<Entry>) -> TestResult {
    let mut batch = WriteBatch::new();
    entries.into_iter().for_each(|e| batch.push(e));
    store.apply(batch, PersistMode::Buffer)?;
    Ok(())
}

#[test]
fn test_entries_carry_the_discard_flag_and_load_reads_one_tenant() -> TestResult {
    let store = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    let a = Catalog::initial(1, id(10)?);
    let b = Catalog::initial(2, id(20)?);
    let entries = a.entries(5)?;
    assert!(entries.iter().all(|e| e.discard() && e.version() == 5));
    write(&store, entries)?;
    write(&store, b.entries(5)?)?;
    let snapshot = store.snapshot(u64::MAX)?;
    assert_eq!(Catalog::load(&snapshot, 1)?, a);
    assert_eq!(Catalog::load(&snapshot, 2)?, b);
    assert_eq!(Catalog::load(&snapshot, 3)?, Catalog::new(3));
    Ok(())
}

#[test]
fn test_an_entry_under_the_wrong_kind_or_undecodable_is_refused() -> TestResult {
    let store = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    let declaration = Declaration::Predicate(PredicateDeclaration::new(NamedNode::new_unchecked(
        "http://e/p",
    )));
    write(
        &store,
        vec![Entry::put(
            Catalog::key(0, ENTITY_TYPE, id(3)?),
            2,
            1,
            encode_declaration(&declaration)?,
        )],
    )?;
    let Err(wrong_kind) = Catalog::load(&store.snapshot(u64::MAX)?, 0) else {
        return Err(TestError::new(
            "load returned Ok where an error was expected",
        ));
    };
    assert_eq!(
        wrong_kind.0,
        CatalogErrorEnum::Store(StoreError(StoreErrorEnum::Corrupted {
            key: Catalog::key(0, ENTITY_TYPE, id(3)?),
            reason: "the entry's kind is not its key's".to_string(),
        }))
    );
    write(
        &store,
        vec![Entry::put(
            Catalog::key(1, PREDICATE, id(3)?),
            2,
            1,
            vec![9],
        )],
    )?;
    let Err(undecodable) = Catalog::load(&store.snapshot(u64::MAX)?, 1) else {
        return Err(TestError::new(
            "load returned Ok where an error was expected",
        ));
    };
    assert_eq!(
        undecodable.0,
        CatalogErrorEnum::Store(StoreError(StoreErrorEnum::Corrupted {
            key: Catalog::key(1, PREDICATE, id(3)?),
            reason: "unknown format 9".to_string(),
        }))
    );
    Ok(())
}

fn ex(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

#[test]
fn test_a_catalog_of_every_kind_reads_back_as_written() -> TestResult {
    let store = SimStore::new(Seed::new(2), FaultConfig::none(), SimClock::new());
    let mut catalog = Catalog::initial(7, id(1)?);
    catalog.declare_predicate(
        id(2)?,
        PredicateDeclaration::new(ex("name")).with_kind(PredicateKind::Property),
    )?;
    catalog.declare_type(
        id(3)?,
        TypeDeclaration::new(ex("Person"), None, false),
        None,
    )?;
    catalog.declare_type(
        id(4)?,
        TypeDeclaration::new(ex("Employee"), Some(ex("Person")), false),
        Some(id(3)?),
    )?;
    catalog.declare_relation(
        id(5)?,
        RelationDeclaration::new(ex("marriage"), vec![ex("a")]),
    )?;
    let entries = catalog.entries(9)?;
    assert_eq!(entries.len(), 5);
    write(&store, entries)?;
    let loaded = Catalog::load(&store.snapshot(u64::MAX)?, 7)?;
    assert_eq!(loaded, catalog);
    assert_eq!(loaded.preorder(), catalog.preorder());
    Ok(())
}

#[test]
fn test_keys_name_the_tenant_the_kind_and_the_id() -> TestResult {
    let predicate = Catalog::predicate_key(3, id(5)?);
    assert_eq!(predicate, Catalog::key(3, PREDICATE, id(5)?));
    for (kind, other) in [(ENTITY_TYPE, RELATION), (RELATION, PREDICATE)] {
        assert_ne!(
            Catalog::key(3, kind, id(5)?),
            Catalog::key(3, other, id(5)?)
        );
    }
    assert_ne!(predicate, Catalog::predicate_key(4, id(5)?));
    assert_ne!(predicate, Catalog::predicate_key(3, id(6)?));
    Ok(())
}

#[test]
fn test_a_stored_entry_is_written_as_a_declarations_entry() -> TestResult {
    let declaration = Declaration::Predicate(PredicateDeclaration::new(ex("p")));
    let entry = Catalog::entry(2, id(4)?, &declaration, 11)?;
    let replayed = Catalog::stored_entry(
        Catalog::predicate_key(2, id(4)?),
        encode_declaration(&declaration)?,
        11,
    );
    assert_eq!(replayed, entry);
    assert!(replayed.discard());
    assert_eq!(replayed.version(), 11);
    Ok(())
}

#[test]
fn test_a_removal_hides_the_declaration_from_the_load() -> TestResult {
    let store = SimStore::new(Seed::new(4), FaultConfig::none(), SimClock::new());
    let catalog = Catalog::initial(1, id(10)?);
    write(&store, catalog.entries(5)?)?;
    write(&store, vec![Catalog::removal(1, PREDICATE, id(10)?, 6)])?;
    assert_eq!(
        Catalog::load(&store.snapshot(u64::MAX)?, 1)?,
        Catalog::new(1)
    );
    // Read before the removal, the declaration is there.
    assert_eq!(Catalog::load(&store.snapshot(5)?, 1)?, catalog);
    Ok(())
}

#[test]
fn test_a_key_under_the_prefix_that_is_no_catalog_key_is_refused() -> TestResult {
    let store = SimStore::new(Seed::new(5), FaultConfig::none(), SimClock::new());
    let declaration = Declaration::Predicate(PredicateDeclaration::new(ex("p")));
    let mut key = Catalog::predicate_key(0, id(3)?);
    key.push(0);
    write(
        &store,
        vec![Entry::put(
            key.clone(),
            2,
            1,
            encode_declaration(&declaration)?,
        )],
    )?;
    let refused = Catalog::load(&store.snapshot(u64::MAX)?, 0)
        .err()
        .ok_or_else(|| TestError::new("a refusal"))?;
    assert_eq!(
        refused.0,
        CatalogErrorEnum::Store(StoreError(StoreErrorEnum::Corrupted {
            key,
            reason: "not a catalog key".to_string(),
        }))
    );
    Ok(())
}
