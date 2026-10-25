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

/// The keys a request read are those it holds through a store read; a list written blind and never read is not
/// one, and becomes one once a read completes it.
#[test]
fn test_the_keys_read_leave_out_lists_written_blind() {
    let lists = new_lists();
    let (read, blind) = (key("read", 1), key("blind", 1));
    let mut request = lists.request(lists.latest_ts());
    request.get(&read).unwrap();
    request
        .get_from_delta(&blind)
        .add_mutation(&set_uid(3), None)
        .unwrap();
    assert_eq!(
        request.read_keys().collect::<Vec<_>>(),
        vec![read.as_slice()]
    );
    request.get(&blind).unwrap();
    assert_eq!(request.read_keys().count(), 2);
}

/// A fork reads as its request does, the batch's earlier writes included, and starts with none of the request's own
/// lists; absorbing it takes its lists and entries back, so parts of one request can be applied apart and committed
/// as one.
#[test]
fn test_a_fork_reads_as_its_request_and_is_absorbed_back() {
    let lists = new_lists();
    let (a, b) = (key("fork", 1), key("fork", 2));
    let mut batch = lists.batch();
    let mut first = batch.request(lists.latest_ts());
    first
        .get(&a)
        .unwrap()
        .add_mutation(&set_uid(1), None)
        .unwrap();
    batch.push(first);
    let mut request = batch.request(lists.latest_ts());
    request.get(&b).unwrap();
    let mut fork = request.fork();
    assert_eq!(fork.read_ts(), request.read_ts());
    assert_eq!(fork.lists().count(), 0);
    let read_ts = fork.read_ts();
    assert_eq!(uids(fork.get(&a).unwrap(), read_ts, 0), vec![1]);
    fork.get(&a)
        .unwrap()
        .add_mutation(&set_uid(2), None)
        .unwrap();
    fork.add_entry(ultragraphdb_traits_store::Entry::deletion(
        key("fork", 3),
        1,
    ));
    request.absorb(fork);
    assert_eq!(request.lists().count(), 2);
    assert_eq!(uids(request.get(&a).unwrap(), read_ts, 0), vec![1, 2]);
    batch.push(request);
    let ts = lists.commit(batch).unwrap();
    assert_eq!(uids(&lists.read(&a, ts[1]).unwrap(), ts[1], 0), vec![1, 2]);
    assert_eq!(
        lists.store().contents().versions(&key("fork", 3))[0].version(),
        ts[1]
    );
}

#[test]
fn test_the_written_keys_are_the_keys_of_the_deltas() {
    let lists = new_lists();
    let (written, read_only, emptied) = (key("friend", 1), key("friend", 2), key("friend", 3));
    commit_edges(&lists, &read_only, &[set_uid(5)], None);
    let mut request = lists.request(lists.latest_ts());
    request
        .get(&written)
        .unwrap()
        .add_mutation(&set_uid(2), None)
        .unwrap();
    request.get(&read_only).unwrap();
    request.get_from_delta(&emptied);
    let keys: Vec<Vec<u8>> = request.written_keys().map(<[u8]>::to_vec).collect();
    let from_deltas: Vec<Vec<u8>> = request.deltas().into_iter().map(|(k, _)| k).collect();
    assert_eq!(keys, from_deltas);
    assert_eq!(
        keys,
        vec![written],
        "a list read or held without a write is not written"
    );
}
