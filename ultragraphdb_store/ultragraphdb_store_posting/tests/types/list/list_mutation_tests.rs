/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_posting::{List, SET};

use crate::utils::helpers::{add, del_uid, key, set_uid, uid_schema, uids};

#[test]
fn test_a_single_uid_predicate_replaces_its_target() {
    let schema = uid_schema(false);
    let mut l = List::empty(key("partner", 1));
    l.set_ts(1);
    l.add_mutation(&set_uid(2), Some(&schema)).unwrap();
    l.commit_mutation(2);
    l.set_ts(3);
    l.add_mutation(&set_uid(3), Some(&schema)).unwrap();
    l.commit_mutation(4);
    assert_eq!(uids(&l, 5, 0), vec![3]);
}

#[test]
fn test_a_uid_list_accumulates() {
    let schema = uid_schema(true);
    let mut l = List::empty(key("friend", 1));
    for (ts, uid) in [(1, 2), (3, 3)] {
        l.set_ts(ts);
        l.add_mutation(&set_uid(uid), Some(&schema)).unwrap();
        l.commit_mutation(ts + 1);
    }
    assert_eq!(uids(&l, 5, 0), vec![2, 3]);
}

#[test]
fn test_with_a_count_index_a_delete_removes_the_requests_set() {
    let mut schema = uid_schema(true);
    schema.count = true;
    let mut l = List::empty(key("counted", 1));
    l.set_ts(1);
    l.add_mutation(&set_uid(7), Some(&schema)).unwrap();
    l.add_mutation(&del_uid(7), Some(&schema)).unwrap();
    assert!(
        l.delta().unwrap().postings.is_empty(),
        "the delete removed the set"
    );
    l.add_mutation(&del_uid(8), Some(&schema)).unwrap();
    l.add_mutation(&del_uid(8), Some(&schema)).unwrap();
    assert_eq!(
        l.delta().unwrap().postings.len(),
        1,
        "a delete of a uid not set is recorded once"
    );
    add(&mut l, 1, &set_uid(9));
    assert_eq!(l.delta().unwrap().postings[1].op, SET);
}
