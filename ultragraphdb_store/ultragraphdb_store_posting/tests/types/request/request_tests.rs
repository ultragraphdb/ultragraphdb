/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_store::{Entry, ListCache};

use crate::utils::helpers::{
    commit_edges, del_uid, key, new_lists, set_uid, set_value, uids, values,
};

#[test]
fn test_a_list_first_written_blind_is_read_whole() {
    let lists = new_lists();
    let k = key("friend", 1);
    commit_edges(&lists, &k, &[set_uid(2)]);
    lists.cache().remove(&k);
    let read_ts = lists.applied_ts();
    let mut request = lists.request(read_ts);
    request
        .get_from_delta(&k)
        .add_posting(set_uid(3).clone(), false);
    assert_eq!(uids(request.get(&k).unwrap(), read_ts, 0), vec![2, 3]);
    // The completed list keeps the write made blind, and later writes go on top.
    request
        .get(&k)
        .unwrap()
        .add_posting(del_uid(2).clone(), false);
    assert_eq!(uids(request.get(&k).unwrap(), read_ts, 0), vec![3]);
}

#[test]
fn test_a_later_request_in_a_batch_reads_an_earlier_one() {
    let lists = new_lists();
    let k = key("name", 1);
    let mut batch = lists.batch();
    let mut first = batch.request(lists.applied_ts());
    first
        .get(&k)
        .unwrap()
        .add_posting(set_value("A").clone(), false);
    batch.push(first);
    let read_ts = lists.applied_ts();
    let mut second = batch.request(read_ts);
    assert_eq!(
        values(second.get(&k).unwrap(), read_ts),
        vec![b"A".to_vec()]
    );
    // A blind write in the later request stacks on the earlier one too.
    let other = key("friend", 9);
    let mut third_batch = lists.batch();
    let mut a = third_batch.request(read_ts);
    a.get_from_delta(&other)
        .add_posting(set_uid(4).clone(), false);
    third_batch.push(a);
    let mut b = third_batch.request(read_ts);
    b.get_from_delta(&other)
        .add_posting(set_uid(5).clone(), false);
    assert_eq!(uids(b.get(&other).unwrap(), read_ts, 0), vec![4, 5]);
    assert_eq!(b.deltas().len(), 1, "only its own write is its delta");
}

#[test]
fn test_another_request_does_not_see_uncommitted_writes() {
    let lists = new_lists();
    let k = key("name", 2);
    let ts = commit_edges(&lists, &k, &[set_value("before")]);
    let mut writer = lists.request(ts);
    writer
        .get(&k)
        .unwrap()
        .add_posting(set_value("after").clone(), false);
    let mut reader = lists.request(ts);
    assert_eq!(
        values(reader.get_scalar_list(&k).unwrap(), ts),
        vec![b"before".to_vec()]
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
    let mut request = lists.request(lists.applied_ts());
    request.get(&read).unwrap();
    request
        .get_from_delta(&blind)
        .add_posting(set_uid(3).clone(), false);
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
    let mut first = batch.request(lists.applied_ts());
    first
        .get(&a)
        .unwrap()
        .add_posting(set_uid(1).clone(), false);
    batch.push(first);
    let mut request = batch.request(lists.applied_ts());
    request.get(&b).unwrap();
    let mut fork = request.fork();
    assert_eq!(fork.read_ts(), request.read_ts());
    assert_eq!(fork.lists().count(), 0);
    let read_ts = fork.read_ts();
    assert_eq!(uids(fork.get(&a).unwrap(), read_ts, 0), vec![1]);
    fork.get(&a).unwrap().add_posting(set_uid(2).clone(), false);
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
    commit_edges(&lists, &read_only, &[set_uid(5)]);
    let mut request = lists.request(lists.applied_ts());
    request
        .get(&written)
        .unwrap()
        .add_posting(set_uid(2).clone(), false);
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

#[test]
fn test_a_list_held_blind_without_a_write_is_read_whole() {
    let lists = new_lists();
    let k = key("blind-unwritten", 1);
    let ts = commit_edges(&lists, &k, &[set_uid(2)]);
    let mut request = lists.request(ts);
    assert!(request.get_from_delta(&k).delta().is_none());
    let list = request.get(&k).unwrap();
    assert_eq!(uids(list, ts, 0), vec![2]);
    assert_eq!(list.mutation().read_ts(), ts);
    assert!(list.delta().is_none());
}

#[test]
fn test_absorbing_a_fork_keeps_the_list_its_writer_holds() {
    let lists = new_lists();
    let (both, mine) = (key("absorb", 1), key("absorb", 2));
    let mut request = lists.request(lists.applied_ts());
    request.get(&both).unwrap();
    request.get(&mine).unwrap().add_posting(set_uid(1), false);
    let mut fork = request.fork();
    fork.get(&both).unwrap().add_posting(set_uid(2), false);
    fork.get(&mine).unwrap().add_posting(set_uid(3), false);
    fork.advance_epoch_on_commit();
    fork.remove_rebuild_record_on_commit(4);
    fork.add_entry_at_version(Entry::deletion(key("absorb", 3), 1));
    fork.add_versionless(b"term".to_vec(), b"uid".to_vec());
    request.absorb(fork);
    let read_ts = request.read_ts();
    // Only the fork wrote `both`, so its list is taken; both wrote `mine`, so the request's own stays.
    assert_eq!(uids(request.get(&both).unwrap(), read_ts, 0), vec![2]);
    assert_eq!(uids(request.get(&mine).unwrap(), read_ts, 0), vec![1]);
    assert_eq!(
        request.entry_keys().collect::<Vec<_>>(),
        vec![key("absorb", 3).as_slice()]
    );
    assert_eq!(request.versionless().len(), 1);
    let epoch = lists.epoch();
    let mut batch = lists.batch();
    batch.push(request);
    lists.commit(batch).unwrap();
    assert_eq!(lists.epoch(), epoch + 1, "the fork's epoch mark is taken");
}

#[test]
fn test_a_requests_entry_keys_are_its_entries_in_order() {
    let lists = new_lists();
    let mut request = lists.request(lists.applied_ts());
    request.add_entry(Entry::deletion(b"\x01a".to_vec(), 0));
    request.add_entry_at_version(Entry::deletion(b"\x01b".to_vec(), 3));
    request.add_entry(Entry::deletion(b"\x01c".to_vec(), 0));
    assert_eq!(
        request.entry_keys().collect::<Vec<_>>(),
        vec![b"\x01a".as_slice(), b"\x01c", b"\x01b"]
    );
    assert!(std::ptr::eq(request.posting_lists(), &lists));
}

#[test]
fn test_a_request_holds_its_read_open_until_it_is_dropped() {
    let lists = new_lists();
    let ts = lists.applied_ts();
    let first = lists.request(ts);
    let second = lists.request(ts);
    assert_eq!(lists.oldest_open_read(), Some(ts));
    drop(first);
    assert_eq!(
        lists.oldest_open_read(),
        Some(ts),
        "the second is still open"
    );
    drop(second);
    assert_eq!(lists.oldest_open_read(), None);
}

#[test]
fn test_a_list_whose_writes_cancelled_out_is_not_written() {
    let lists = new_lists();
    let k = key("cancelled", 1);
    let mut request = lists.request(lists.applied_ts());
    let list = request.get_from_delta(&k);
    list.add_posting(set_uid(7), true);
    list.add_posting(del_uid(7), true);
    assert_eq!(list.delta().map(|d| d.postings().len()), Some(0));
    assert!(request.deltas().is_empty());
    assert_eq!(request.written_keys().count(), 0);
}

#[test]
fn test_absorbing_keeps_an_epoch_mark_that_both_carry() {
    let lists = new_lists();
    let epoch = lists.epoch();
    let mut request = lists.request(lists.applied_ts());
    request.advance_epoch_on_commit();
    let mut fork = request.fork();
    fork.advance_epoch_on_commit();
    request.absorb(fork);
    let mut batch = lists.batch();
    batch.push(request);
    lists.commit(batch).unwrap();
    assert_eq!(lists.epoch(), epoch + 1);
}

/// A tested statement is recorded by its list key and object, and the list it read is not a read key: an insert's
/// existence check conflicts only with a commit changing that object.
#[test]
fn test_a_tested_statement_is_recorded_and_its_list_is_not_read_whole() {
    let lists = new_lists();
    let k = key("knows", 1);
    commit_edges(&lists, &k, &[set_uid(2)]);
    let read_ts = lists.applied_ts();
    let mut request = lists.request(read_ts);
    assert_eq!(
        uids(request.get_tested(&k, b"o3").unwrap(), read_ts, 0),
        vec![2]
    );
    assert_eq!(
        request.tested().collect::<Vec<_>>(),
        vec![(k.as_slice(), b"o3".as_slice())]
    );
    assert_eq!(request.read_keys().count(), 0);
}

/// A list tested and then read whole is a read key, and its test stays recorded.
#[test]
fn test_a_list_read_whole_after_a_test_is_a_read_key() {
    let lists = new_lists();
    let k = key("knows", 2);
    commit_edges(&lists, &k, &[set_uid(2)]);
    let mut request = lists.request(lists.applied_ts());
    request.get_tested(&k, b"o").unwrap();
    request.get(&k).unwrap();
    assert_eq!(request.read_keys().collect::<Vec<_>>(), vec![k.as_slice()]);
    assert_eq!(request.tested().count(), 1);
}

/// A test after a whole read leaves the list read whole.
#[test]
fn test_a_test_after_a_whole_read_keeps_the_list_read_whole() {
    let lists = new_lists();
    let k = key("knows", 3);
    let mut request = lists.request(lists.applied_ts());
    request.get(&k).unwrap();
    request.get_tested(&k, b"o").unwrap();
    assert_eq!(request.read_keys().collect::<Vec<_>>(), vec![k.as_slice()]);
}

/// A test of a list written blind completes it from the store, its writes kept on top, and still reads no list
/// whole.
#[test]
fn test_a_test_completes_a_list_written_blind() {
    let lists = new_lists();
    let k = key("knows", 4);
    commit_edges(&lists, &k, &[set_uid(2)]);
    lists.cache().remove(&k);
    let read_ts = lists.applied_ts();
    let mut request = lists.request(read_ts);
    request.get_from_delta(&k).add_posting(set_uid(3), false);
    assert_eq!(
        uids(request.get_tested(&k, b"o").unwrap(), read_ts, 0),
        vec![2, 3]
    );
    assert_eq!(request.read_keys().count(), 0);
    assert_eq!(
        request.written_keys().collect::<Vec<_>>(),
        vec![k.as_slice()]
    );
}

/// Each statement is recorded once, in key order then object order, whatever order and however often it was tested.
#[test]
fn test_tested_statements_are_recorded_once_in_order() {
    let lists = new_lists();
    let (low, high) = (key("knows", 5), key("knows", 6));
    assert!(low < high, "the keys sort by subject");
    let mut request = lists.request(lists.applied_ts());
    request.get_tested(&high, b"b").unwrap();
    request.get_tested(&low, b"z").unwrap();
    request.get_tested(&high, b"a").unwrap();
    request.get_tested(&high, b"b").unwrap();
    assert_eq!(
        request.tested().collect::<Vec<_>>(),
        vec![
            (low.as_slice(), b"z".as_slice()),
            (high.as_slice(), b"a".as_slice()),
            (high.as_slice(), b"b".as_slice()),
        ]
    );
}

/// A scan returns the keys of the range the store holds at the request's read timestamp and those the request wrote,
/// in key order and once each, and records the range.
#[test]
fn test_a_scan_returns_stored_and_written_keys_of_its_range_and_records_it() {
    let lists = new_lists();
    let keys: Vec<Vec<u8>> = (1..=5).map(|s| key("scan", s)).collect();
    for k in [&keys[0], &keys[1], &keys[3]] {
        commit_edges(&lists, k, &[set_uid(9)]);
    }
    let mut request = lists.request(lists.applied_ts());
    request
        .get_from_delta(&keys[2])
        .add_posting(set_uid(1), false);
    request
        .get(&keys[3])
        .unwrap()
        .add_posting(set_uid(2), false);
    request
        .get_from_delta(&keys[4])
        .add_posting(set_uid(1), false);
    let found = request
        .scan(keys[1].clone(), Some(keys[4].clone()))
        .unwrap();
    // keys[0] lies before the range and keys[4], written, at its end, which is exclusive.
    assert_eq!(
        found,
        vec![keys[1].clone(), keys[2].clone(), keys[3].clone()]
    );
    assert_eq!(
        request.ranges().collect::<Vec<_>>(),
        vec![(keys[1].as_slice(), Some(keys[4].as_slice()))]
    );
}

/// A scan without an end runs to the last key.
#[test]
fn test_a_scan_without_an_end_runs_to_the_last_key() {
    let lists = new_lists();
    let (a, b) = (key("open", 1), key("open", 2));
    commit_edges(&lists, &a, &[set_uid(1)]);
    commit_edges(&lists, &b, &[set_uid(1)]);
    let mut request = lists.request(lists.applied_ts());
    assert_eq!(request.scan(b.clone(), None).unwrap(), vec![b.clone()]);
    assert_eq!(
        request.ranges().collect::<Vec<_>>(),
        vec![(b.as_slice(), None)]
    );
}

/// An empty range finds nothing and is recorded all the same: a commit can still write a key there only if the range
/// holds one, so it conflicts with nothing, and the record costs nothing.
#[test]
fn test_an_empty_range_finds_nothing_and_is_recorded() {
    let lists = new_lists();
    let a = key("empty", 1);
    commit_edges(&lists, &a, &[set_uid(1)]);
    let mut request = lists.request(lists.applied_ts());
    assert!(request.scan(a.clone(), Some(a.clone())).unwrap().is_empty());
    assert_eq!(request.ranges().count(), 1);
}

/// A scan reads the store at the request's read timestamp: a key committed after it is not found.
#[test]
fn test_a_scan_reads_the_store_at_the_read_timestamp() {
    let lists = new_lists();
    let (a, b) = (key("later", 1), key("later", 2));
    commit_edges(&lists, &a, &[set_uid(1)]);
    let mut request = lists.request(lists.applied_ts());
    commit_edges(&lists, &b, &[set_uid(1)]);
    let found = request.scan(key("later", 0), None).unwrap();
    assert_eq!(found, vec![a]);
}

/// A list the request emptied is still a key of the store at its read timestamp; one it wrote and cancelled out, and
/// that the store does not hold, is no key at all.
#[test]
fn test_a_list_written_and_cancelled_out_is_not_found() {
    let lists = new_lists();
    let (held, cancelled) = (key("cancel", 1), key("cancel", 2));
    commit_edges(&lists, &held, &[set_uid(1)]);
    let mut request = lists.request(lists.applied_ts());
    request.get(&held).unwrap().add_posting(del_uid(1), false);
    let list = request.get_from_delta(&cancelled);
    list.add_posting(set_uid(7), true);
    list.add_posting(del_uid(7), true);
    assert_eq!(request.scan(key("cancel", 0), None).unwrap(), vec![held]);
}

/// A range recorded by the caller's own scan is reported with the request's scans, each range once, in order.
#[test]
fn test_a_recorded_range_is_reported_once_with_the_scans() {
    let lists = new_lists();
    let mut request = lists.request(lists.applied_ts());
    request.record_range(b"\x10b".to_vec(), Some(b"\x10c".to_vec()));
    request.record_range(b"\x10a".to_vec(), None);
    request.record_range(b"\x10b".to_vec(), Some(b"\x10c".to_vec()));
    request
        .scan(b"\x10a".to_vec(), Some(b"\x10b".to_vec()))
        .unwrap();
    assert_eq!(
        request.ranges().collect::<Vec<_>>(),
        vec![
            (b"\x10a".as_slice(), None),
            (b"\x10a".as_slice(), Some(b"\x10b".as_slice())),
            (b"\x10b".as_slice(), Some(b"\x10c".as_slice())),
        ]
    );
    assert_eq!(
        request.read_keys().count(),
        0,
        "recording a range reads no list"
    );
}

/// Absorbing a fork takes its tested statements and ranges, and a list either read whole stays read whole.
#[test]
fn test_absorbing_a_fork_keeps_its_tests_ranges_and_whole_reads() {
    let lists = new_lists();
    let (tested, scanned) = (key("absorbed", 1), key("absorbed", 2));
    let mut request = lists.request(lists.applied_ts());
    request.get_tested(&tested, b"mine").unwrap();
    request
        .get(&scanned)
        .unwrap()
        .add_posting(set_uid(1), false);
    let mut fork = request.fork();
    fork.get(&tested).unwrap();
    fork.get_tested(&tested, b"theirs").unwrap();
    fork.record_range(b"\x10r".to_vec(), None);
    request.absorb(fork);
    assert_eq!(
        request.read_keys().collect::<Vec<_>>(),
        vec![tested.as_slice(), scanned.as_slice()]
    );
    assert_eq!(
        request.tested().collect::<Vec<_>>(),
        vec![
            (tested.as_slice(), b"mine".as_slice()),
            (tested.as_slice(), b"theirs".as_slice()),
        ]
    );
    assert_eq!(
        request.ranges().collect::<Vec<_>>(),
        vec![(b"\x10r".as_slice(), None)]
    );
}
