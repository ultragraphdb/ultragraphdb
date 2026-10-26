/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/mvcc_test.go (TestPostingListRead, TestCacheAfterDeltaUpdateRecieved,
 * TestCacheStaleWhenMaxTsLessThanReadTs). Changed for ultragraphdb: translated to Rust; the cache tests are adapted
 * from ristretto's timing to the port's cache, whose entries every commit through the writer replaces, so the
 * stale entry Go's second test builds by skipping `UpdateCachedKeys` cannot arise, and the test checks instead that
 * the commit replaced the entry; the rest of the file tests the port's read path and cache rule.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::sync::Arc;

use ultragraphdb_proto_pb::pb::PostingList;
use ultragraphdb_store_posting::{
    BIT_COMPLETE_POSTING, BIT_EMPTY_POSTING, CachedList, List, ListOptions, ListsConfig,
    PostingErrorEnum, marshal,
};
use ultragraphdb_traits_store::{Entry, ListCache, Store};

use crate::utils::helpers::{
    TestLists, commit_edges, delete_all, key, lists_over, new_lists, new_store, next_commit_at,
    set_uid, uids, write_delta, write_entries,
};

fn uid_count(lists: &TestLists, key: &[u8], read_ts: u64) -> usize {
    // Go reads the newest list and asks it for the uids at the read timestamp.
    let snapshot = lists.store().snapshot(u64::MAX).expect("snapshot");
    let (list, _) = List::read(key, &snapshot).expect("read");
    list.uids(&ListOptions::at(read_ts))
        .expect("uids")
        .uids
        .len()
}

#[test]
fn test_posting_list_read() {
    let lists = new_lists();
    let k = key("emptypl", 1);
    write_delta(lists.store(), &k, 2, &[set_uid(2)]);
    write_delta(lists.store(), &k, 4, &[set_uid(3)]);
    write_entries(
        lists.store(),
        vec![Entry::put(k.clone(), 6, BIT_EMPTY_POSTING, Vec::new())],
    );
    assert_eq!(uid_count(&lists, &k, 7), 0);

    write_delta(lists.store(), &k, 8, &[set_uid(4)]);
    assert_eq!(uid_count(&lists, &k, 9), 1);

    write_entries(
        lists.store(),
        vec![Entry::put(
            k.clone(),
            10,
            BIT_COMPLETE_POSTING,
            marshal(&PostingList::default()),
        )],
    );
    assert_eq!(uid_count(&lists, &k, 10), 0);

    write_delta(lists.store(), &k, 12, &[set_uid(5)]);
    write_delta(lists.store(), &k, 14, &[set_uid(6)]);
    write_delta(lists.store(), &k, 16, &[set_uid(7)]);
    assert_eq!(uid_count(&lists, &k, 17), 3);
}

#[test]
fn test_a_read_below_an_empty_rollup_reads_the_older_versions() {
    // Deviation from Dgraph, which serves the empty list to every read below it (mvcc.go:708-709).
    let store = new_store();
    let k = key("emptybelow", 1);
    write_delta(&store, &k, 4, &[set_uid(9)]);
    write_delta(&store, &k, 5, &[delete_all()]);
    write_entries(
        &store,
        vec![Entry::put(k.clone(), 6, BIT_EMPTY_POSTING, Vec::new()).with_discard()],
    );
    let lists = lists_over(store, ListsConfig::default());
    let list = lists.read(&k, 4).expect("read");
    assert_eq!(uids(&list, 4, 0), vec![9]);
    let newest = lists.read(&k, 7).expect("read");
    assert!(uids(&newest, 7, 0).is_empty());
    // The empty version is the list's complete version: a read below it through that list is refused.
    let newest = lists.read_uncached(&k, 7).expect("read");
    assert!(matches!(
        newest.uids(&ListOptions::at(5)).unwrap_err().0,
        PostingErrorEnum::ReadBelowMinTs { min_ts: 6, .. }
    ));
}

#[test]
fn test_cache_after_delta_update_received() {
    let lists = new_lists();
    let k = ultragraphdb_core_keys::QuadKey::ValueIndex {
        namespace: 0,
        graph: ultragraphdb_core_keys::DEFAULT_GRAPH,
        predicate: crate::utils::helpers::predicate("cache"),
        tokenizer: 1,
        token: b"temp".to_vec(),
    }
    .to_bytes();
    next_commit_at(&lists, 15);
    assert_eq!(commit_edges(&lists, &k, &[set_uid(1)]), 15);

    // A read at 10 does not see the commit, and a read at 20 does.
    let l = lists.read(&k, 10).expect("read");
    assert_eq!(l.mutation().list_len(10), 0);
    let l1 = lists.read(&k, 20).expect("read");
    assert_eq!(l1.mutation().list_len(20), 1);
}

#[test]
fn test_cache_is_replaced_by_a_commit_after_its_fill() {
    // Go's TestCacheStaleWhenMaxTsLessThanReadTs builds a cache older than the store by skipping the cache update;
    // every commit of the port replaces the entry, so the read at 25 is served from the cache and sees uid 2.
    let lists = new_lists();
    let k = key("issue9597", 1);
    next_commit_at(&lists, 10);
    assert_eq!(commit_edges(&lists, &k, &[set_uid(1)]), 10);
    let l1 = lists.read(&k, 10).expect("read");
    assert_eq!(l1.mutation().list_len(10), 1);
    assert!(lists.cache().get(&k).is_some(), "the read fills the cache");

    next_commit_at(&lists, 20);
    assert_eq!(commit_edges(&lists, &k, &[set_uid(2)]), 20);
    let entry = lists
        .cache()
        .get(&k)
        .expect("the commit replaced the entry");
    assert_eq!(entry.ts(), 20);

    let l2 = lists.cached(&k, 25).expect("read").expect("a cache hit");
    assert_eq!(l2.uids(&ListOptions::at(25)).unwrap().uids, vec![1, 2]);
}

#[test]
fn test_a_read_above_the_lists_newest_version_hits() {
    let lists = new_lists();
    let k = key("hits", 1);
    let ts = commit_edges(&lists, &k, &[set_uid(1)]);
    lists.read(&k, ts).expect("fill");
    // Dgraph misses here: its hit needs the read at or below the list's newest version (issue #9597).
    assert!(lists.cached(&k, ts + 100).expect("read").is_some());
}

#[test]
fn test_a_stale_fill_is_refused() {
    let lists = new_lists();
    let k = key("stalefill", 1);
    let first = commit_edges(&lists, &k, &[set_uid(1)]);
    lists.cache().remove(&k);
    let miss = lists.miss(&k, first).expect("miss");
    let second = commit_edges(&lists, &k, &[set_uid(2)]);
    assert!(!lists.fill(miss), "a commit wrote the key after the miss");
    // The entry is the second commit's, which replaced the one its own request filled.
    assert_eq!(lists.cache().get(&k).map(|e| e.ts()), Some(second));
    let read = lists.read(&k, second).expect("read");
    assert_eq!(uids(&read, second, 0), vec![1, 2]);
}

#[test]
fn test_a_fill_of_another_key_survives_a_commit() {
    let lists = new_lists();
    let (a, b) = (key("fills", 1), key("fills", 2));
    let ts = commit_edges(&lists, &a, &[set_uid(1)]);
    let miss = lists.miss(&a, ts).expect("miss");
    commit_edges(&lists, &b, &[set_uid(2)]);
    assert!(lists.fill(miss));
    assert_eq!(lists.cache().get(&a).expect("filled").ts(), ts);
}

#[test]
fn test_a_fill_begun_before_an_epoch_change_is_refused() {
    let lists = new_lists();
    let k = key("epochfill", 1);
    let ts = commit_edges(&lists, &k, &[set_uid(1)]);
    let miss = lists.miss(&k, ts).expect("miss");
    lists.advance_epoch();
    assert!(!lists.fill(miss));
}

#[test]
fn test_an_entry_of_an_older_epoch_is_a_miss() {
    let lists = new_lists();
    let k = key("epochentry", 1);
    let ts = commit_edges(&lists, &k, &[set_uid(1)]);
    lists.read(&k, ts).expect("fill");
    assert!(lists.cached(&k, ts).expect("read").is_some());
    lists.advance_epoch();
    assert!(lists.cached(&k, ts).expect("read").is_none());
}

#[test]
fn test_an_entry_serves_reads_from_its_timestamp() {
    let list = Arc::new(List::new(key("serves", 1), PostingList::default(), 3));
    let entry = CachedList::new(list, 5, 2);
    assert!(entry.serves(5, 2) && entry.serves(9, 2));
    assert!(!entry.serves(4, 2), "below its timestamp");
    assert!(!entry.serves(9, 3), "another epoch");
}

#[test]
fn test_a_miss_reads_below_a_newer_complete_version_again() {
    let lists = new_lists();
    let k = key("missbelow", 1);
    let first = commit_edges(&lists, &k, &[set_uid(1)]);
    let second = commit_edges(&lists, &k, &[set_uid(2)]);
    assert!(lists.rollup_key(&k).expect("rollup"));
    // A commit elsewhere moves the applied timestamp above the rollup's version.
    commit_edges(&lists, &key("missbelow", 2), &[set_uid(3)]);
    let miss = lists.miss(&k, first).expect("miss");
    assert_eq!(uids(miss.list(), first, 0), vec![1]);
    assert!(miss.base() >= second);
    assert_eq!(miss.epoch(), lists.epoch());
    assert_eq!(uids(&miss.into_list(), first, 0), vec![1]);
}

#[test]
fn test_reads_with_deltas_queue_their_key_for_a_rollup() {
    let lists = new_lists();
    let k = key("queued", 1);
    let ts = commit_edges(&lists, &k, &[set_uid(1)]);
    let before = lists.rollups_waiting();
    lists.read_uncached(&k, ts).expect("read");
    assert_eq!(lists.rollups_waiting(), before + 1);
}
