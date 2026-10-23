/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Tests of the port of Dgraph v25.5.0, worker/mutation.go (runSchemaMutation, checkSchema) and posting/index.go
 * (the rebuilds). Changed for ultragraphdb: written for the port; Dgraph tests these through its cluster tests.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_keys::predicate_prefix;
use ultragraphdb_core_schema::SchemaMode;
use ultragraphdb_core_tok::{VEC_ENTRY, VEC_KEYWORD};
use ultragraphdb_proto_pb::pb::{SchemaUpdate, posting};

use crate::utils::helpers::{
    alter, alter_now, attr, ckey, dkey, mutate, new_applier, rkey, set_str, set_uid, stored_keys,
    uids,
};

#[test]
fn test_a_change_while_an_index_is_built_is_refused() {
    let a = new_applier();
    alter_now(&a, "v: string .");
    mutate(&a, vec![set_str("v", 1, "x")]).unwrap();
    alter(&a, "v: string @index(exact) .").unwrap();
    let err = alter(&a, "w: string .").unwrap_err();
    assert_eq!(err.to_string(), "errIndexingInProgress. Please retry");
    a.executor().run_until_idle();
    alter(&a, "w: string .").unwrap();
}

/// Every update of a change is checked before any is applied: a change refused by its second update applies
/// neither.
#[test]
fn test_a_refused_change_alters_nothing() {
    let a = new_applier();
    alter_now(&a, "secret: string .");
    let err = alter(&a, "fresh: string .\nsecret: password .").unwrap_err();
    assert_eq!(
        err.to_string(),
        "Schema change not allowed from STRING to PASSWORD"
    );
    assert!(a.schema().get(SchemaMode::Query, &attr("fresh")).is_none());
}

/// Dgraph's `checkSchema` refusals of an update itself.
#[test]
fn test_check_schema_refuses_ill_formed_updates() {
    let a = new_applier();
    let su = |f: fn(&mut SchemaUpdate)| {
        let mut s = SchemaUpdate {
            predicate: attr("p"),
            value_type: posting::ValType::String as i32,
            ..SchemaUpdate::default()
        };
        f(&mut s);
        s
    };
    let text = |s: SchemaUpdate| a.alter(vec![s]).unwrap_err().to_string();
    assert_eq!(
        text(su(|s| s.predicate = attr(""))),
        "No predicate specified in schema mutation"
    );
    assert_eq!(
        text(su(|s| s.predicate = attr("uid"))),
        "Cannot create user-defined predicate with internal name uid"
    );
    assert!(
        text(su(|s| s.directive = 1)).starts_with(
            "Tokenizer must be specified while indexing a predicate: predicate:\"0-p\""
        )
    );
    assert_eq!(
        text(su(|s| s.tokenizer = vec!["exact".into()])),
        "Directive must be SchemaUpdate_INDEX when a tokenizer is specified"
    );
    assert_eq!(
        text(su(|s| s.directive = 2)),
        "Cannot reverse for non-uid type on predicate p"
    );
    assert_eq!(
        text(su(|s| s.upsert = true)),
        "Index tokenizer is mandatory for: [p] when specifying @upsert directive"
    );
    assert_eq!(
        text(su(|s| s.unique = true)),
        "index for predicate [p] is missing, add either hash or exact index with @unique"
    );
    assert_eq!(
        text(su(|s| {
            s.value_type = posting::ValType::Uid as i32;
            s.directive = 1;
            s.tokenizer = vec!["exact".into()];
        })),
        "Index not allowed on predicate of type uid on predicate p"
    );
    assert!(a.schema().predicates().is_empty(), "nothing applied");
}

/// A `@unique` predicate gains `@upsert`; dropping its index is refused.
#[test]
fn test_unique_keeps_its_index() {
    let a = new_applier();
    alter_now(&a, "id: string @index(hash) @unique .");
    let s = a.schema().get(SchemaMode::Query, &attr("id")).unwrap();
    assert!(s.upsert && s.unique);
    let err = alter(&a, "id: string @index(term) @unique @upsert .").unwrap_err();
    assert_eq!(
        err.to_string(),
        "could not drop index [hash] from [id] predicate when @unique directive specified"
    );
}

/// A count index added to a predicate holding data is built, its reverse counts with it, and every data list is
/// rewritten whole at the change's timestamp, as Dgraph's rebuild of a `@count` predicate does.
#[test]
fn test_a_count_index_is_built_with_its_reverse_counts() {
    let a = new_applier();
    alter_now(&a, "friend: [uid] @reverse .");
    mutate(
        &a,
        vec![
            set_uid("friend", 1, 2),
            set_uid("friend", 1, 3),
            set_uid("friend", 4, 2),
        ],
    )
    .unwrap();
    let change_ts = alter_now(&a, "friend: [uid] @reverse @count .");
    assert_eq!(uids(&a, &ckey("friend", 2, false)), vec![1]);
    assert_eq!(uids(&a, &ckey("friend", 1, false)), vec![4]);
    assert_eq!(uids(&a, &ckey("friend", 2, true)), vec![2]);
    assert_eq!(uids(&a, &ckey("friend", 1, true)), vec![3]);
    let versions = a
        .lists()
        .store()
        .contents()
        .versions(&dkey("friend", 1))
        .to_vec();
    assert_eq!(
        versions[0].version(),
        change_ts,
        "rewritten whole at the change"
    );
    // Maintained after the build.
    mutate(&a, vec![set_uid("friend", 4, 3)]).unwrap();
    assert_eq!(uids(&a, &ckey("friend", 2, false)), vec![1, 4]);
    assert_eq!(uids(&a, &ckey("friend", 2, true)), vec![2, 3]);
}

/// Removing reverse edges deletes the reverse keys; adding them back rebuilds them.
#[test]
fn test_reverse_edges_are_removed_and_rebuilt() {
    let a = new_applier();
    alter_now(&a, "friend: [uid] @reverse .");
    mutate(&a, vec![set_uid("friend", 1, 2)]).unwrap();
    assert_eq!(uids(&a, &rkey("friend", 2)), vec![1]);
    alter_now(&a, "friend: [uid] .");
    assert!(uids(&a, &rkey("friend", 2)).is_empty());
    alter_now(&a, "friend: [uid] @reverse .");
    assert_eq!(uids(&a, &rkey("friend", 2)), vec![1]);
}

/// A vector index added to a predicate holding vectors is built in place, before the change returns, every vector
/// inserted; removed, its predicates are deleted.
#[test]
fn test_a_vector_index_is_built_in_place_and_removed() {
    let a = new_applier();
    alter_now(&a, "emb: float32vector .");
    let edges = (1..=10u64)
        .map(|uid| set_str("emb", uid, &format!("[{uid}, 1]")))
        .collect();
    mutate(&a, edges).unwrap();
    alter(
        &a,
        "emb: float32vector @index(hnsw(maxLevels:\"2\", efConstruction:\"6\", efSearch:\"6\")) .",
    )
    .unwrap();
    assert!(!a.rebuilding(), "built before the change returned");
    let vec_key = attr(&format!("emb{VEC_KEYWORD}"));
    for uid in 1..=10u64 {
        let key = ultragraphdb_core_keys::data_key(&vec_key, uid).unwrap();
        assert!(!uids(&a, &key).is_empty(), "{uid} has neighbour lists");
    }
    let entry = ultragraphdb_core_keys::data_key(&attr(&format!("emb{VEC_ENTRY}")), 1).unwrap();
    assert!(!uids(&a, &entry).is_empty());
    alter_now(&a, "emb: float32vector .");
    for p in [format!("emb{VEC_KEYWORD}"), format!("emb{VEC_ENTRY}")] {
        let prefix = predicate_prefix(&attr(&p)).unwrap();
        assert!(
            crate::utils::helpers::live_keys(&a, &prefix).is_empty(),
            "{p} deleted"
        );
        assert!(!stored_keys(&a, &prefix).is_empty(), "deleted, not dropped");
    }
}

/// A rebuild cancelled by a drop of its predicate writes nothing when its build ends.
#[test]
fn test_a_drop_cancels_a_rebuild() {
    let a = new_applier();
    alter_now(&a, "v: string .");
    mutate(&a, vec![set_str("v", 1, "x")]).unwrap();
    alter(&a, "v: string @index(exact) .").unwrap();
    a.drop_predicate(&attr("v")).unwrap();
    a.executor().run_until_idle();
    assert!(!a.rebuilding());
    assert!(stored_keys(&a, &predicate_prefix(&attr("v")).unwrap()).is_empty());
    assert!(a.schema().get(SchemaMode::Query, &attr("v")).is_none());
}

/// A rebuild that fails a value the new type cannot hold restores the previous schema, as Dgraph's does.
#[test]
fn test_a_value_the_new_type_cannot_hold_fails_the_build() {
    let a = new_applier();
    alter_now(&a, "n: string @index(exact) .");
    mutate(&a, vec![set_str("n", 1, "seven")]).unwrap();
    alter(&a, "n: int @index(int) .").unwrap();
    a.executor().run_until_idle();
    let s = a.schema().get(SchemaMode::Query, &attr("n")).unwrap();
    assert_eq!(s.value_type, posting::ValType::String as i32);
    assert!(!a.schema().indexing_in_progress());
    assert!(!a.rebuilding());
    assert_eq!(s.tokenizer, ["exact"]);
    // The previous index stays usable, and a later change can run.
    assert_eq!(
        uids(&a, &crate::utils::helpers::ikey("n", "exact", "seven")),
        vec![1]
    );
    alter_now(&a, "n: string @index(exact, term) .");
    assert_eq!(
        uids(&a, &crate::utils::helpers::ikey("n", "term", "seven")),
        vec![1]
    );
}

/// Writes during a count rebuild move the counts the build makes: the build's lists at the change's timestamp, the
/// writes' deltas above them.
#[test]
fn test_writes_during_a_count_rebuild_move_its_counts() {
    let a = new_applier();
    alter_now(&a, "friend: [uid] @reverse .");
    mutate(&a, vec![set_uid("friend", 9, 4), set_uid("friend", 9, 5)]).unwrap();
    alter(&a, "friend: [uid] @reverse @count .").unwrap();
    mutate(&a, vec![set_uid("friend", 1, 4)]).unwrap();
    a.executor().run_until_idle();
    assert_eq!(
        uids(&a, &ckey("friend", 2, true)),
        vec![4],
        "4 has two sources"
    );
    assert_eq!(uids(&a, &ckey("friend", 1, true)), vec![5]);
    assert_eq!(uids(&a, &ckey("friend", 2, false)), vec![9]);
    assert_eq!(uids(&a, &ckey("friend", 1, false)), vec![1]);
}

/// Deviation: every HNSW option below 1 is refused when the schema change carrying it arrives; Go accepts it and
/// panics at the first search. An exponent of 0 sets the levels it leaves unset to 0.
#[test]
fn test_hnsw_options_below_one_are_refused_at_the_change() {
    let a = new_applier();
    for (option, name) in [
        ("maxLevels:\"0\"", "maxLevels"),
        ("efConstruction:\"0\"", "efConstruction"),
        ("efSearch:\"-1\"", "efSearch"),
        ("exponent:\"0\"", "maxLevels"),
    ] {
        let err = alter(&a, &format!("emb: float32vector @index(hnsw({option})) .")).unwrap_err();
        assert!(
            err.to_string()
                .starts_with(&format!("IndexFactory: hnsw issues this error: 'the vector index option {name} must be at least 1")),
            "{option}: {err}"
        );
    }
    assert!(a.schema().get(SchemaMode::Query, &attr("emb")).is_none());
}
