/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Tests of the port of Dgraph v25.5.0, posting/index.go (the dead list's update in addIndexMutations). Changed for
 * ultragraphdb: written for the port; Dgraph has no test of the dead list.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_tok::VEC_DEAD;
use ultragraphdb_core_types::Value;
use ultragraphdb_store_hnsw::parse_edges;

use crate::utils::helpers::{TestApplier, alter_now, del_str, dkey, mutate, new_applier, set_str};

/// The uids of the dead list of `emb`.
fn dead(a: &TestApplier) -> Vec<u64> {
    let key = dkey(&format!("emb{VEC_DEAD}"), 1);
    let ts = a.lists().latest_ts();
    match a.lists().read(&key, ts).unwrap().value(ts) {
        Ok(v) => match v.value() {
            Some(Value::Bytes(b)) => parse_edges(&String::from_utf8_lossy(b)).unwrap(),
            _ => Vec::new(),
        },
        Err(_) => Vec::new(),
    }
}

/// Deviation (ledger 53): Dgraph appends a deleted vector's uid to the dead list and never removes it, so a vector
/// set again after its delete stays dead and later inserts leave it out of their neighbour lists
/// (`posting/index.go:113-148`); the port's set removes the uid from the dead list.
#[test]
fn test_a_vector_deleted_and_set_again_leaves_the_dead_list() {
    let a = new_applier();
    alter_now(&a, "emb: float32vector @index(hnsw) .");
    for uid in 1..=4u64 {
        mutate(&a, vec![set_str("emb", uid, &format!("[{uid}, 1]"))]).unwrap();
    }
    mutate(&a, vec![del_str("emb", 2, "[2, 1]")]).unwrap();
    mutate(&a, vec![del_str("emb", 3, "[3, 1]")]).unwrap();
    assert_eq!(dead(&a), vec![2, 3]);
    mutate(&a, vec![set_str("emb", 2, "[20, 1]")]).unwrap();
    assert_eq!(
        dead(&a),
        vec![3],
        "the uid set again is live; the other stays dead"
    );
}

/// A set of a uid the dead list does not hold leaves the list as it is.
#[test]
fn test_a_set_of_a_live_vector_leaves_the_dead_list_alone() {
    let a = new_applier();
    alter_now(&a, "emb: float32vector @index(hnsw) .");
    mutate(&a, vec![set_str("emb", 1, "[1, 1]")]).unwrap();
    mutate(&a, vec![del_str("emb", 1, "[1, 1]")]).unwrap();
    let before = a.lists().latest_ts();
    mutate(&a, vec![set_str("emb", 2, "[2, 1]")]).unwrap();
    let key = dkey(&format!("emb{VEC_DEAD}"), 1);
    let list = a.lists().read(&key, a.lists().latest_ts()).unwrap();
    assert!(list.max_ts() <= before, "no write to the dead list");
    assert_eq!(dead(&a), vec![1]);
}
