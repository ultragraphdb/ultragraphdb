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
use ultragraphdb_traits_store::{Entry, PersistMode, Store, WriteBatch};

fn write(store: &SimStore, entries: Vec<Entry>) {
    let mut batch = WriteBatch::new();
    entries.into_iter().for_each(|e| batch.push(e));
    store.apply(batch, PersistMode::Buffer).unwrap();
}

#[test]
fn test_entries_carry_the_discard_flag_and_load_reads_one_namespace() {
    let store = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    let mut a = Catalog::initial(1, 10);
    assert!(a.mark_statements(11) && !a.mark_statements(11) && a.has_statements(11));
    let b = Catalog::initial(2, 20);
    let entries = a.entries(5);
    assert!(entries.iter().all(|e| e.discard() && e.version() == 5));
    write(&store, entries);
    write(&store, b.entries(5));
    let snapshot = store.snapshot(u64::MAX).unwrap();
    assert_eq!(Catalog::load(&snapshot, 1).unwrap(), a);
    assert_eq!(Catalog::load(&snapshot, 2).unwrap(), b);
    assert_eq!(Catalog::load(&snapshot, 3).unwrap(), Catalog::new(3));
}

#[test]
fn test_an_entry_under_the_wrong_kind_or_undecodable_is_refused() {
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
    );
    let error = Catalog::load(&store.snapshot(u64::MAX).unwrap(), 0).unwrap_err();
    assert!(matches!(
        error.0,
        CatalogErrorEnum::Store(StoreError(StoreErrorEnum::Corrupted { .. }))
    ));
    write(
        &store,
        vec![Entry::put(Catalog::key(1, PREDICATE, 3), 2, 1, vec![9])],
    );
    let error = Catalog::load(&store.snapshot(u64::MAX).unwrap(), 1).unwrap_err();
    assert!(error.to_string().contains("unknown format 9"), "{error}");
}
