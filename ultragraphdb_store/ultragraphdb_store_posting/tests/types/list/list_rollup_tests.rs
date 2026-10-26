/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::split_key;
use ultragraphdb_store_posting::{BIT_COMPLETE_POSTING, List};
use ultragraphdb_traits_store::Store;

use crate::utils::helpers::{
    add, del_uid, key, new_store, read_back, roll_up, set_uid, uids, with_facet,
};

#[test]
fn test_a_rollup_deletes_the_parts_it_removes() {
    let store = new_store();
    let k = key("split-delete", 1);
    let mut l = read_back(&store, &k);
    (1..=2000).for_each(|u| add(&mut l, 1, &set_uid(u)));
    l.commit_mutation(2);
    let (l, version) = roll_up(&store, &l, u64::MAX, 2000);
    let splits = l.part_splits();
    assert!(splits.len() > 1);
    // Every part is written at the main key's version.
    for start in &splits {
        let part = store.contents().versions(&split_key(&k, *start).unwrap())[0].clone();
        assert_eq!(
            (part.version(), part.meta(), part.discard()),
            (version, BIT_COMPLETE_POSTING, true)
        );
    }

    let mut l = l;
    (1..=2000).for_each(|u| add(&mut l, 3, &del_uid(u)));
    l.commit_mutation(4);
    let (_, entries) = l.rollup_entries(5, 2000).unwrap().unwrap();
    let deletions: Vec<_> = entries.iter().filter(|e| e.is_deletion()).collect();
    assert_eq!(
        deletions.len(),
        splits.len(),
        "every part is deleted when the list empties"
    );
    assert!(entries.iter().all(|e| e.version() == 5));
    crate::utils::helpers::write_entries(&store, entries);
    let snapshot = store.snapshot(10).unwrap();
    for start in &splits {
        let part = split_key(&k, *start).unwrap();
        assert!(
            ultragraphdb_traits_store::StoreSnapshot::get(&snapshot, &part)
                .unwrap()
                .is_none()
        );
    }
    let emptied = read_back(&store, &k);
    assert!(emptied.part_splits().is_empty());
    assert!(uids(&emptied, 10, 0).is_empty());
}

#[test]
fn test_a_rollup_below_the_complete_version_writes_nothing() {
    let l = List::new(key("below", 1), Default::default(), 9);
    assert!(l.rollup_entries(5, 1000).unwrap().is_none());
}

#[test]
fn test_iteration_after_a_uid_crosses_part_boundaries() {
    let store = new_store();
    let k = key("cross", 1);
    let mut l = read_back(&store, &k);
    (1..=3000).for_each(|u| add(&mut l, 1, &with_facet(set_uid(u), "f")));
    l.commit_mutation(2);
    let (l, _) = roll_up(&store, &l, u64::MAX, 4096);
    assert!(l.part_splits().len() > 2);
    assert_eq!(uids(&l, 3, 10), (11..=3000).collect::<Vec<_>>());
    assert!(l.approx_len() >= 3000);
}

#[test]
fn test_a_rollup_keeps_the_statement_ids_of_a_uid_posting() {
    use std::ops::ControlFlow;
    use ultragraphdb_core_keys::QuadKey;
    use ultragraphdb_proto_pb::pb::Posting;
    use ultragraphdb_store_posting::{PostingError, SET};

    let store = new_store();
    let k = QuadKey::Forward {
        namespace: 0,
        graph: 0,
        predicate: 2,
        subject: 1,
    }
    .to_bytes();
    let mut l = read_back(&store, &k);
    l.set_ts(1);
    for (uid, statement_ids) in [(7, vec![3, 4]), (8, Vec::new())] {
        let posting = Posting {
            uid,
            op: SET,
            statement_ids,
            ..Posting::default()
        };
        l.add_posting(posting, false);
    }
    l.commit_mutation(2);
    let (l, _) = roll_up(&store, &l, u64::MAX, 2000);
    let mut held = Vec::new();
    l.iterate(10, 0, |p| {
        held.push((p.uid, p.statement_ids.clone()));
        Ok::<_, PostingError>(ControlFlow::Continue(()))
    })
    .unwrap();
    assert_eq!(held, vec![(7, vec![3, 4]), (8, Vec::new())]);
}
