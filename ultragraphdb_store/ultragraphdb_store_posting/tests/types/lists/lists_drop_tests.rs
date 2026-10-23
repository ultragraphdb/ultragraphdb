/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_posting::BIT_SCHEMA_POSTING;
use ultragraphdb_traits_store::{Entry, Store};

use crate::utils::helpers::{commit_edges, key, new_lists, set_uid, uids};

#[test]
fn test_a_drop_advances_the_epoch_and_clears_the_cache() {
    let lists = new_lists();
    let k = key("dropped", 1);
    let ts = commit_edges(&lists, &k, &[set_uid(1)], None);
    lists.read(&k, ts).unwrap();
    let epoch = lists.epoch();
    lists
        .drop_prefixes(lists.fresh_ts(), std::slice::from_ref(&k), Vec::new())
        .unwrap();
    assert_eq!(lists.epoch(), epoch + 1);
    assert!(lists.cached(&k, ts).unwrap().is_none());
    let after = lists.fresh_ts();
    assert!(uids(&lists.read(&k, after).unwrap(), after, 0).is_empty());
}

#[test]
fn test_a_drop_of_everything_keeps_its_entries() {
    let lists = new_lists();
    let ts = commit_edges(&lists, &key("all", 1), &[set_uid(1)], None);
    lists
        .drop_all(vec![Entry::put(
            b"\x01s".to_vec(),
            1,
            BIT_SCHEMA_POSTING,
            b"x".to_vec(),
        )])
        .unwrap();
    let contents = lists.store().contents();
    assert_eq!(contents.keys().len(), 1);
    assert!(lists.cached(&key("all", 1), ts).unwrap().is_none());
}

#[test]
fn test_the_discard_timestamp_stays_at_or_below_the_oldest_open_request() {
    let lists = new_lists();
    let ts = commit_edges(&lists, &key("discard", 1), &[set_uid(1)], None);
    let request = lists.request(ts);
    assert_eq!(lists.oldest_open_read(), Some(ts));
    lists.set_discard_ts(ts + 100).unwrap();
    assert!(lists.store().discard_ts() <= ts);
    drop(request);
    assert_eq!(lists.oldest_open_read(), None);
    let later = lists.fresh_ts();
    lists.set_discard_ts(later).unwrap();
    assert_eq!(lists.store().discard_ts(), later);
}
