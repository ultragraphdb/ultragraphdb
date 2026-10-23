/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_store::ListCache;

use crate::utils::helpers::{commit_edges, del_uid, key, new_lists, set_uid, set_value, uids};

#[test]
fn test_a_list_first_written_blind_is_read_whole() {
    let lists = new_lists();
    let k = key("friend", 1);
    commit_edges(&lists, &k, &[set_uid(2)], None);
    lists.cache().remove(&k);
    let read_ts = lists.latest_ts();
    let mut request = lists.request(read_ts);
    request
        .get_from_delta(&k)
        .add_mutation(&set_uid(3), None)
        .unwrap();
    assert_eq!(uids(request.get(&k).unwrap(), read_ts, 0), vec![2, 3]);
    // The completed list keeps the write made blind, and later writes go on top.
    request
        .get(&k)
        .unwrap()
        .add_mutation(&del_uid(2), None)
        .unwrap();
    assert_eq!(uids(request.get(&k).unwrap(), read_ts, 0), vec![3]);
}

#[test]
fn test_a_later_request_in_a_batch_reads_an_earlier_one() {
    let lists = new_lists();
    let k = key("name", 1);
    let mut batch = lists.batch();
    let mut first = batch.request(lists.latest_ts());
    first
        .get(&k)
        .unwrap()
        .add_mutation(&set_value("A"), None)
        .unwrap();
    batch.push(first);
    let read_ts = lists.latest_ts();
    let mut second = batch.request(read_ts);
    let value = second.get(&k).unwrap().value(read_ts).unwrap();
    assert_eq!(
        value.value(),
        Some(&ultragraphdb_core_types::Value::Bytes(b"A".to_vec()))
    );
    // A blind write in the later request stacks on the earlier one too.
    let other = key("friend", 9);
    let mut third_batch = lists.batch();
    let mut a = third_batch.request(read_ts);
    a.get_from_delta(&other)
        .add_mutation(&set_uid(4), None)
        .unwrap();
    third_batch.push(a);
    let mut b = third_batch.request(read_ts);
    b.get_from_delta(&other)
        .add_mutation(&set_uid(5), None)
        .unwrap();
    assert_eq!(uids(b.get(&other).unwrap(), read_ts, 0), vec![4, 5]);
    assert_eq!(b.deltas().len(), 1, "only its own write is its delta");
}

#[test]
fn test_another_request_does_not_see_uncommitted_writes() {
    let lists = new_lists();
    let k = key("name", 2);
    let ts = commit_edges(&lists, &k, &[set_value("before")], None);
    let mut writer = lists.request(ts);
    writer
        .get(&k)
        .unwrap()
        .add_mutation(&set_value("after"), None)
        .unwrap();
    let mut reader = lists.request(ts);
    let seen = reader.get_scalar_list(&k).unwrap().value(ts).unwrap();
    assert_eq!(
        seen.value(),
        Some(&ultragraphdb_core_types::Value::Bytes(b"before".to_vec()))
    );
    assert_eq!(writer.read_ts(), ts);
    assert_eq!(writer.lists().count(), 1);
}
