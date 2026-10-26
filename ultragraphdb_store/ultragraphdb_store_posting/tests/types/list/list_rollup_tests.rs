/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_posting::split_key_of;
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
        let part = store
            .contents()
            .versions(&split_key_of(&k, *start).unwrap())[0]
            .clone();
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
        let part = split_key_of(&k, *start).unwrap();
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
    // Hand-derived: 3000 uids fill 12 blocks of 256, which splitting keeps whole; the approximate length is 12 × 256.
    assert_eq!(l.approx_len(), 3072);
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

#[test]
fn test_a_rollup_at_the_complete_version_is_made_and_one_below_is_not() {
    let l = List::new(key("at-min", 1), Default::default(), 9);
    assert!(l.rollup(8, true, 1000).unwrap().is_none());
    let out = l
        .rollup(9, true, 1000)
        .unwrap()
        .expect("at the complete version");
    assert_eq!(out.new_min_ts(), 9);
    let (version, entries) = l.rollup_entries(9, 1000).unwrap().unwrap();
    assert_eq!(
        version, 10,
        "a finite rollup writes one above the newest version it read"
    );
    assert_eq!(
        (entries.len(), entries[0].meta(), entries[0].discard()),
        (1, ultragraphdb_store_posting::BIT_EMPTY_POSTING, true)
    );
}

#[test]
fn test_a_rollup_one_below_the_maximum_timestamp_is_finite() {
    let mut l = List::empty(key("near-max", 1));
    add(&mut l, 3, &set_uid(1));
    l.commit_mutation(4);
    assert_eq!(l.rollup_entries(u64::MAX - 1, 1000).unwrap().unwrap().0, 5);
    assert_eq!(l.rollup_entries(u64::MAX, 1000).unwrap().unwrap().0, 4);
}

#[test]
fn test_a_rollup_takes_the_deltas_at_or_below_its_timestamp() {
    let store = new_store();
    let mut l = List::empty(key("partial", 1));
    add(&mut l, 1, &set_uid(1));
    l.commit_mutation(2);
    add(&mut l, 3, &set_uid(2));
    l.commit_mutation(4);
    let (version, entries) = l.rollup_entries(3, 1000).unwrap().unwrap();
    assert_eq!(version, 3, "the newest commit read is 2");
    crate::utils::helpers::write_entries(&store, entries);
    let rolled = read_back(&store, l.key());
    assert_eq!((rolled.min_ts(), uids(&rolled, 3, 0)), (3, vec![1]));
}

#[test]
fn test_a_rollup_keeps_the_newest_version_of_a_posting() {
    let store = new_store();
    let mut l = List::empty(key("newest-version", 1));
    add(&mut l, 1, &with_facet(set_uid(5), "old"));
    l.commit_mutation(2);
    add(&mut l, 3, &with_facet(set_uid(5), "new"));
    l.commit_mutation(4);
    let (rolled, version) = roll_up(&store, &l, u64::MAX, 2000);
    assert_eq!(version, 4);
    assert_eq!(rolled.plist().postings.len(), 1);
    assert_eq!(rolled.plist().postings[0].facets[0].key, "new");
    assert_eq!(rolled.plist().postings[0].commit_ts, 4);
}

#[test]
fn test_a_list_without_deltas_rolls_up_into_itself() {
    let store = new_store();
    let mut l = List::empty(key("itself", 1));
    [2, 4].iter().for_each(|&u| add(&mut l, 1, &set_uid(u)));
    l.commit_mutation(2);
    let (rolled, _) = roll_up(&store, &l, u64::MAX, 2000);
    let out = rolled.rollup(7, true, 2000).unwrap().unwrap();
    assert_eq!(out.plist(), rolled.plist());
    assert_eq!(out.new_min_ts(), rolled.min_ts());
    assert!(out.parts().is_empty());
}

#[test]
fn test_a_rollup_without_splitting_makes_one_list_of_a_split_one() {
    let store = new_store();
    let k = key("unsplit-backup", 1);
    let mut l = read_back(&store, &k);
    (1..=2000).for_each(|u| add(&mut l, 1, &set_uid(u)));
    l.commit_mutation(2);
    let (l, _) = roll_up(&store, &l, u64::MAX, 2000);
    assert!(l.part_splits().len() > 1);
    let out = l.rollup(u64::MAX, false, 2000).unwrap().unwrap();
    assert!(out.plist().splits.is_empty() && out.parts().is_empty());
    assert_eq!(
        ultragraphdb_core_codec::decode(out.plist().pack.as_ref().unwrap(), 0),
        (1..=2000).collect::<Vec<_>>()
    );
}

/// A rollup of a split list whose middle part was emptied, and the first uids of the part after it deleted, must
/// still put each uid in the part whose range holds it; a read after a uid in the later part then finds every uid
/// above it.
#[test]
fn test_a_rollup_puts_each_uid_in_its_part_when_a_part_empties() {
    let store = new_store();
    let k = key("emptied-middle", 1);
    let mut l = read_back(&store, &k);
    (1..=3000).for_each(|u| add(&mut l, 1, &set_uid(u)));
    l.commit_mutation(2);
    let (mut l, _) = roll_up(&store, &l, u64::MAX, 1000);
    let splits = l.part_splits();
    assert!(splits.len() > 3, "{splits:?}");
    let (a, b) = (splits[1], splits[2]);
    // Every uid of the part starting at a, and the first six of the part starting at b.
    (a..=b + 5).for_each(|u| add(&mut l, 3, &del_uid(u)));
    l.commit_mutation(4);
    let (rolled, _) = roll_up(&store, &l, u64::MAX, 1000);
    let above: Vec<u64> = (b + 6..=3000).collect();
    let read = uids(&rolled, 5, b + 1);
    assert!(
        read == above,
        "a read after {} starts at {:?}, not {}",
        b + 1,
        read.first(),
        b + 6
    );
    assert_eq!(
        rolled.find_posting(5, b + 6).unwrap().map(|p| p.uid),
        Some(b + 6)
    );
    let all: Vec<u64> = (1..a).chain(b + 6..=3000).collect();
    assert!(uids(&rolled, 5, 0) == all, "every uid but those deleted");
    // Every part holds only uids of its own range.
    let splits = rolled.part_splits();
    for (i, part) in rolled.parts().iter().enumerate() {
        let end = splits.get(i + 1).map_or(u64::MAX, |next| next - 1);
        let held = ultragraphdb_core_codec::decode(part.pack.as_ref().unwrap(), 0);
        assert!(
            held.iter().all(|&u| (splits[i]..=end).contains(&u)),
            "part {} holds {:?}..{:?}",
            splits[i],
            held.first(),
            held.last()
        );
    }
}
