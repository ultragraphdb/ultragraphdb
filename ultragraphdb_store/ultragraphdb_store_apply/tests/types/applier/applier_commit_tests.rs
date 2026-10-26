/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The checks at commit: a request that read below a drop or schema change it writes under is refused, and with
//! it every later request of its batch that read a list it wrote; a request whose new predicate gained a schema is
//! converted to it. Run on the simulated store.

use ultragraphdb_core_keys::{predicate_prefix, schema_key};
use ultragraphdb_core_catalog::SchemaMode;
use ultragraphdb_proto_pb::pb::posting;

use crate::utils::helpers::{
    alter, alter_now, attr, dkey, ikey, mutate, new_applier, next_commit_at, set_str, set_uid,
    stored_keys, uids,
};

/// A request refused at commit takes with it the later requests of its batch that read what it wrote: their reads
/// stacked on its writes, which are never written.
#[test]
fn test_a_refused_request_takes_its_dependents_with_it() {
    let a = new_applier();
    alter_now(&a, "name: string @index(exact) .\nfriend: [uid] .");
    mutate(&a, vec![set_str("name", 9, "Z")]).unwrap();
    next_commit_at(&a, 10);
    let mut batch = a.batch();
    // Reads below the change of `name` made next, and writes under it.
    let mut stale = batch.request(9);
    stale.run_mutation(&set_str("name", 1, "A")).unwrap();
    batch.push(stale);
    let change_ts = alter(&a, "name: string @index(exact) @count .").unwrap();
    a.executor().run_until_idle();
    let read_ts = a.lists().latest_ts();
    assert!(read_ts >= change_ts);
    // Reads what the stale request wrote.
    let mut dependent = batch.request(read_ts);
    dependent.request().get(&dkey("name", 1)).unwrap();
    dependent.run_mutation(&set_uid("friend", 1, 2)).unwrap();
    batch.push(dependent);
    // Reads nothing the stale request wrote.
    let mut independent = batch.request(read_ts);
    independent.run_mutation(&set_uid("friend", 3, 4)).unwrap();
    batch.push(independent);
    let results = batch.commit().unwrap();
    assert!(results[0].as_ref().unwrap_err().is_aborted());
    assert!(results[1].as_ref().unwrap_err().is_aborted(), "{results:?}");
    assert!(results[2].is_ok());
    assert!(uids(&a, &dkey("name", 1)).is_empty());
    assert!(uids(&a, &dkey("friend", 1)).is_empty());
    assert_eq!(uids(&a, &dkey("friend", 3)), vec![4]);
}

/// A request that read before a schema change of a predicate it writes is refused: its index entries were made
/// under the schema before the change.
#[test]
fn test_a_request_that_read_before_a_schema_change_fails() {
    let a = new_applier();
    alter_now(&a, "name: string .");
    let batch_ts = a.lists().latest_ts();
    let mut batch = a.batch();
    let mut request = batch.request(batch_ts);
    request.run_mutation(&set_str("name", 1, "A")).unwrap();
    batch.push(request);
    alter_now(&a, "name: string @index(exact) .");
    let err = batch.commit().unwrap().remove(0).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Transaction has been aborted. Please retry"
    );
    assert!(uids(&a, &ikey("name", "exact", "A")).is_empty());
    // Read after the change, the same write commits.
    mutate(&a, vec![set_str("name", 1, "A")]).unwrap();
    assert_eq!(uids(&a, &ikey("name", "exact", "A")), vec![1]);
}

/// A request that defined a new predicate is refused when a schema change defined it after the request read: the
/// change is a fence, the commit of another request's definition is not.
#[test]
fn test_a_definition_after_the_read_refuses_the_request() {
    let a = new_applier();
    let read_ts = a.lists().latest_ts();
    let mut batch = a.batch();
    let mut request = batch.request(read_ts);
    let m = ultragraphdb_proto_pb::pb::Mutations {
        start_ts: read_ts,
        edges: vec![set_str("nick", 1, "Al")],
        ..Default::default()
    };
    request.apply_edges(&m).unwrap();
    assert!(request.created().contains_key(&attr("nick")));
    batch.push(request);
    alter_now(&a, "nick: string @index(exact) .");
    assert!(batch.commit().unwrap().remove(0).unwrap_err().is_aborted());
    let s = a.schema().get(SchemaMode::Query, &attr("nick")).unwrap();
    assert_eq!(s.tokenizer, ["exact"]);
    assert!(stored_keys(&a, &predicate_prefix(&attr("nick")).unwrap()).is_empty());
}

/// Two requests of one batch creating one predicate: the first defines it, the second's values are converted to
/// its definition, and one schema entry is written.
#[test]
fn test_a_batch_defines_a_predicate_once() {
    let a = new_applier();
    let read_ts = a.lists().latest_ts();
    let mut batch = a.batch();
    for (uid, value) in [(1u64, "x"), (2, "y")] {
        let mut request = batch.request(read_ts);
        let m = ultragraphdb_proto_pb::pb::Mutations {
            start_ts: read_ts,
            edges: vec![set_str("tag", uid, value)],
            ..Default::default()
        };
        request.apply_edges(&m).unwrap();
        batch.push(request);
    }
    let results = batch.commit().unwrap();
    assert!(results.iter().all(Result::is_ok), "{results:?}");
    let versions = a
        .lists()
        .store()
        .contents()
        .versions(&schema_key(&attr("tag")).unwrap())
        .to_vec();
    assert_eq!(versions.len(), 1, "one definition written");
    let s = a.schema().get(SchemaMode::Query, &attr("tag")).unwrap();
    assert_eq!(s.value_type, posting::ValType::Default as i32);
    assert_eq!(uids(&a, &dkey("tag", 2)), vec![u64::MAX]);
}
