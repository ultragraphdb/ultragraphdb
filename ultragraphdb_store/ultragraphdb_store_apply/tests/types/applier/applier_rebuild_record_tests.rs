/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The rebuild record: synced before a build starts, naming the prefixes writes maintain while it builds (the new
//! schema's and the previous schema's); removed by the swap batch, the failure batch, or the batch of a cancelled
//! rebuild; finished at open by dropping its prefixes the loaded schema does not declare.

use ultragraphdb_core_schema::SchemaMode;
use ultragraphdb_store_apply::prefixes_to_delete_tokens_for;
use ultragraphdb_store_sim::Fault;
use ultragraphdb_traits_store::{RebuildRecord, Store};

use crate::utils::helpers::{
    alter, alter_now, applier_over, attr, ikey, index_prefix, live_keys, mutate, new_applier,
    set_str, stored_keys, uids,
};

/// The prefixes of `name`'s exact index, its language identifier's and their split twins.
fn exact_prefixes(name: &str) -> Vec<Vec<u8>> {
    let mut p = prefixes_to_delete_tokens_for(&attr(name), "exact", false).unwrap();
    p.extend(prefixes_to_delete_tokens_for(&attr(name), "exact", true).unwrap());
    p
}

/// An applier whose `name` is `string @index(term)` with three subjects.
fn indexed() -> std::sync::Arc<crate::utils::helpers::TestApplier> {
    let a = new_applier();
    alter_now(&a, "name: string @index(term) .");
    for (uid, v) in [(1, "big apple"), (2, "red apple"), (3, "green pear")] {
        mutate(&a, vec![set_str("name", uid, v)]).unwrap();
    }
    a
}

#[test]
fn test_a_rebuild_records_its_new_prefixes_before_it_builds_and_its_swap_removes_it() {
    let a = indexed();
    alter(&a, "name: string @index(term, exact) .").unwrap();
    let records = a.lists().store().rebuild_records().unwrap();
    assert_eq!(
        records.len(),
        1,
        "the record is written before the build runs"
    );
    let mut declared = prefixes_to_delete_tokens_for(&attr("name"), "term", false).unwrap();
    declared.extend(exact_prefixes("name"));
    assert_eq!(records[0].prefixes(), declared, "the new schema's prefixes");
    a.executor().run_until_idle();
    assert!(a.lists().store().rebuild_records().unwrap().is_empty());
    assert_eq!(uids(&a, &ikey("name", "exact", "red apple")), vec![2]);
}

#[test]
fn test_a_failed_rebuild_removes_its_record() {
    let a = indexed();
    alter(&a, "name: string @index(term, exact) .").unwrap();
    a.lists().store().inject(Fault::FailNextScanAfter(1));
    a.executor().run_until_idle();
    assert!(a.lists().store().rebuild_records().unwrap().is_empty());
    let schema = a.schema().get(SchemaMode::Query, &attr("name")).unwrap();
    assert_eq!(schema.tokenizer, ["term"]);
}

#[test]
fn test_a_cancelled_rebuild_removes_its_record() {
    let a = indexed();
    alter(&a, "name: string @index(term, exact) .").unwrap();
    a.drop_predicate(&attr("name")).unwrap();
    a.executor().run_until_idle();
    assert!(a.lists().store().rebuild_records().unwrap().is_empty());
}

/// A crash while the exact index is built leaves the keys writes maintained for it; the reopened applier loads the
/// previous schema and drops them, keeping the term index whole.
#[test]
fn test_an_unfinished_rebuild_leaves_nothing_of_its_new_indexes_at_open() {
    let a = indexed();
    alter(&a, "name: string @index(term, exact) .").unwrap();
    mutate(&a, vec![set_str("name", 9, "green fig")]).unwrap();
    assert_eq!(uids(&a, &ikey("name", "exact", "green fig")), vec![9]);
    a.lists().sync().unwrap();
    let store = a.lists().store().clone();
    store.crash();
    store.recover();
    let reopened = applier_over(store);
    let schema = reopened
        .schema()
        .get(SchemaMode::Query, &attr("name"))
        .unwrap();
    assert_eq!(schema.tokenizer, ["term"]);
    for prefix in exact_prefixes("name") {
        assert!(
            stored_keys(&reopened, &prefix).is_empty(),
            "the exact index is dropped"
        );
    }
    assert_eq!(uids(&reopened, &ikey("name", "term", "green")), vec![3, 9]);
    assert!(
        reopened
            .lists()
            .store()
            .rebuild_records()
            .unwrap()
            .is_empty()
    );
}

/// At open, a record's prefixes the loaded schema declares are kept: only the undeclared ones are dropped.
#[test]
fn test_at_open_a_records_declared_prefixes_are_kept() {
    let a = indexed();
    let term = prefixes_to_delete_tokens_for(&attr("name"), "term", false).unwrap();
    let mut prefixes = term.clone();
    prefixes.extend(exact_prefixes("name"));
    a.lists()
        .store()
        .write_rebuild_record(&RebuildRecord::new(77, prefixes))
        .unwrap();
    let store = a.lists().store().clone();
    store.crash();
    store.recover();
    let reopened = applier_over(store);
    assert_eq!(live_keys(&reopened, &index_prefix("name")).len(), 5);
    assert!(
        reopened
            .lists()
            .store()
            .rebuild_records()
            .unwrap()
            .is_empty()
    );
}
