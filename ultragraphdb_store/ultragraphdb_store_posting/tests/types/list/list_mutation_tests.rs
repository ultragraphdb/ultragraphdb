/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_posting::{List, OVR, SET};

use crate::utils::helpers::{add, del_uid, key, set_uid, uids};

#[test]
fn test_uid_postings_accumulate() {
    let mut l = List::empty(key("friend", 1));
    for (ts, uid) in [(1, 2), (3, 3)] {
        l.set_ts(ts);
        l.add_posting(set_uid(uid), false);
        l.commit_mutation(ts + 1);
    }
    assert_eq!(uids(&l, 5, 0), vec![2, 3]);
}

#[test]
fn test_with_a_count_index_a_delete_removes_the_requests_set() {
    let mut l = List::empty(key("counted", 1));
    l.set_ts(1);
    l.add_posting(set_uid(7), true);
    l.add_posting(del_uid(7), true);
    assert!(
        l.delta().unwrap().postings().is_empty(),
        "the delete removed the set"
    );
    l.add_posting(del_uid(8), true);
    l.add_posting(del_uid(8), true);
    assert_eq!(
        l.delta().unwrap().postings().len(),
        1,
        "a delete of a uid not set is recorded once"
    );
    add(&mut l, 1, &set_uid(9));
    assert_eq!(l.delta().unwrap().postings()[1].op(), SET);
}

/// Deviation: with a count index, Dgraph's `insertPosting` replaces a set the scratch layer holds by the overwrite
/// count maintenance makes of a second set of the same uid (`list.go:375-394`, `index.go:557-560`), and an overwrite
/// adds nothing to the length (`getLengthDelta`): the list's length then misses the uid, and the counts maintained
/// from it go wrong. The port keeps the set, the overwrite's value and facets taken.
#[test]
fn test_an_overwrite_of_a_set_in_the_scratch_layer_stays_a_set() {
    let mut list = List::empty(key("friend", 1));
    list.set_ts(5);
    list.add_posting(set_uid(3), true);
    list.add_posting(set_uid(3).with_op(OVR), true);
    assert_eq!(list.get_length(5).unwrap(), 1);
    assert_eq!(list.delta().unwrap().postings()[0].op(), SET);
    list.commit_mutation(6);
    assert_eq!(list.get_length(6).unwrap(), 1);
}
