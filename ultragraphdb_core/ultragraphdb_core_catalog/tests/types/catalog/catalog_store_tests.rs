/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::NamedNode;
use ultragraphdb_core_catalog::{
    Catalog, CatalogErrorEnum, ENTITY_TYPE, PREDICATE, encode_declaration,
};
use ultragraphdb_core_mapping::{Declaration, PredicateDeclaration};
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_sim::{FaultConfig, Seed, SimClock, SimStore};
use ultragraphdb_tool_bdd::{TestError, TestResult};
use ultragraphdb_traits_store::{Entry, PersistMode, Store, WriteBatch};

fn write(store: &SimStore, entries: Vec<Entry>) -> TestResult {
    let mut batch = WriteBatch::new();
    entries.into_iter().for_each(|e| batch.push(e));
    store.apply(batch, PersistMode::Buffer)?;
    Ok(())
}

#[test]
fn test_entries_carry_the_discard_flag_and_load_reads_one_namespace() -> TestResult {
    let store = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    let a = Catalog::initial(1, 10);
    let b = Catalog::initial(2, 20);
    let entries = a.entries(5);
    assert!(entries.iter().all(|e| e.discard() && e.version() == 5));
    write(&store, entries)?;
    write(&store, b.entries(5))?;
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
            Catalog::key(0, ENTITY_TYPE, 3),
            2,
            1,
            encode_declaration(&declaration),
        )],
    )?;
    let Err(wrong_kind) = Catalog::load(&store.snapshot(u64::MAX)?, 0) else {
        return Err(TestError::new(
            "load returned Ok where an error was expected",
        ));
    };
    assert!(matches!(
        wrong_kind.0,
        CatalogErrorEnum::Store(StoreError(StoreErrorEnum::Corrupted { .. }))
    ));
    write(
        &store,
        vec![Entry::put(Catalog::key(1, PREDICATE, 3), 2, 1, vec![9])],
    )?;
    let Err(undecodable) = Catalog::load(&store.snapshot(u64::MAX)?, 1) else {
        return Err(TestError::new(
            "load returned Ok where an error was expected",
        ));
    };
    assert!(
        undecodable.to_string().contains("unknown format 9"),
        "{undecodable}"
    );
    Ok(())
}
