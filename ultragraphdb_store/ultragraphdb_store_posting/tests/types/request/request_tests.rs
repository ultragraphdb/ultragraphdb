/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_posting::{ListSource, WalkBounds, WalkEnd, WalkStep};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_store::{Entry, ListCache};

use crate::utils::helpers::{
    check_uids, commit, commit_edges, del_uid, key, new_lists, rids, set_uid, set_value, uids,
    values,
};

#[test]
fn test_a_list_first_written_blind_is_read_whole() -> TestResult {
    let lists = new_lists();
    let k = key("friend", 1);
    commit(&lists, &k, &[set_uid(2)])?;
    lists.cache().remove(&k);
    let read_ts = lists.applied_ts();
    let mut request = lists.request(read_ts);
    request
        .get_from_delta(&k)
        .add_posting(set_uid(3).clone(), false);
    assert_eq!(uids(request.get(&k)?, read_ts, 0)?, rids(&[2, 3]));
    // The completed list keeps the write made blind, and later writes go on top.
    request.get(&k)?.add_posting(del_uid(2).clone(), false);
    assert_eq!(uids(request.get(&k)?, read_ts, 0)?, rids(&[3]));
    Ok(())
}

#[test]
fn test_a_later_request_in_a_batch_reads_an_earlier_one() -> TestResult {
    let lists = new_lists();
    let k = key("name", 1);
    let mut batch = lists.batch();
    let mut first = batch.request(lists.applied_ts());
    first.get(&k)?.add_posting(set_value("A").clone(), false);
    batch.push(first);
    let read_ts = lists.applied_ts();
    let mut second = batch.request(read_ts);
    assert_eq!(values(second.get(&k)?, read_ts)?, vec![b"A".to_vec()]);
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
    assert_eq!(uids(b.get(&other)?, read_ts, 0)?, rids(&[4, 5]));
    assert_eq!(b.deltas().len(), 1, "only its own write is its delta");
    Ok(())
}

#[test]
fn test_another_request_does_not_see_uncommitted_writes() -> TestResult {
    let lists = new_lists();
    let k = key("name", 2);
    let ts = commit_edges(&lists, &k, &[set_value("before")])?;
    let mut writer = lists.request(ts);
    writer
        .get(&k)?
        .add_posting(set_value("after").clone(), false);
    let mut reader = lists.request(ts);
    assert_eq!(
        values(reader.get_scalar_list(&k)?, ts)?,
        vec![b"before".to_vec()]
    );
    assert_eq!(writer.read_ts(), ts);
    assert_eq!(writer.lists().count(), 1);
    Ok(())
}

/// The keys a request read are those it holds through a store read; a list written blind and never read is not
/// one, and becomes one once a read completes it.
#[test]
fn test_the_keys_read_leave_out_lists_written_blind() -> TestResult {
    let lists = new_lists();
    let (read, blind) = (key("read", 1), key("blind", 1));
    let mut request = lists.request(lists.applied_ts());
    let _read_list: &mut ultragraphdb_store_posting::List = request.get(&read)?;
    request
        .get_from_delta(&blind)
        .add_posting(set_uid(3).clone(), false);
    assert_eq!(
        request.read_keys().collect::<Vec<_>>(),
        vec![read.as_slice()]
    );
    let _blind_list: &mut ultragraphdb_store_posting::List = request.get(&blind)?;
    assert_eq!(request.read_keys().count(), 2);
    Ok(())
}

/// A fork reads as its request does, the batch's earlier writes included, and starts with none of the request's own
/// lists; absorbing it takes its lists and entries back, so parts of one request can be applied apart and committed
/// as one.
#[test]
fn test_a_fork_reads_as_its_request_and_is_absorbed_back() -> TestResult {
    let lists = new_lists();
    let (a, b) = (key("fork", 1), key("fork", 2));
    let mut batch = lists.batch();
    let mut first = batch.request(lists.applied_ts());
    first.get(&a)?.add_posting(set_uid(1).clone(), false);
    batch.push(first);
    let mut request = batch.request(lists.applied_ts());
    let _b_list: &mut ultragraphdb_store_posting::List = request.get(&b)?;
    let mut fork = request.fork();
    assert_eq!(fork.read_ts(), request.read_ts());
    assert_eq!(fork.lists().count(), 0);
    let read_ts = fork.read_ts();
    assert_eq!(uids(fork.get(&a)?, read_ts, 0)?, rids(&[1]));
    fork.get(&a)?.add_posting(set_uid(2).clone(), false);
    fork.add_entry(Entry::deletion(key("fork", 3), 1));
    request.absorb(fork);
    assert_eq!(request.lists().count(), 2);
    assert_eq!(uids(request.get(&a)?, read_ts, 0)?, rids(&[1, 2]));
    batch.push(request);
    let ts = lists.commit(batch)?;
    assert_eq!(uids(&lists.read(&a, ts[1])?, ts[1], 0)?, rids(&[1, 2]));
    assert_eq!(
        lists.store().contents().versions(&key("fork", 3))[0].version(),
        ts[1]
    );
    Ok(())
}

#[test]
fn test_the_written_keys_are_the_keys_of_the_deltas() -> TestResult {
    let lists = new_lists();
    let (written, read_only, emptied) = (key("friend", 1), key("friend", 2), key("friend", 3));
    commit(&lists, &read_only, &[set_uid(5)])?;
    let mut request = lists.request(lists.applied_ts());
    request
        .get(&written)?
        .add_posting(set_uid(2).clone(), false);
    let _read_only_list: &mut ultragraphdb_store_posting::List = request.get(&read_only)?;
    let _emptied_list: &mut ultragraphdb_store_posting::List = request.get_from_delta(&emptied);
    let keys: Vec<Vec<u8>> = request.written_keys().map(<[u8]>::to_vec).collect();
    let from_deltas: Vec<Vec<u8>> = request.deltas().into_iter().map(|(k, _)| k).collect();
    assert_eq!(keys, from_deltas);
    assert_eq!(
        keys,
        vec![written],
        "a list read or held without a write is not written"
    );
    Ok(())
}

#[test]
fn test_a_list_held_blind_without_a_write_is_read_whole() -> TestResult {
    let lists = new_lists();
    let k = key("blind-unwritten", 1);
    let ts = commit_edges(&lists, &k, &[set_uid(2)])?;
    let mut request = lists.request(ts);
    assert!(request.get_from_delta(&k).delta().is_none());
    let list = request.get(&k)?;
    assert_eq!(uids(list, ts, 0)?, rids(&[2]));
    assert_eq!(list.mutation().read_ts(), ts);
    assert!(list.delta().is_none());
    Ok(())
}

#[test]
fn test_absorbing_a_fork_keeps_the_list_its_writer_holds() -> TestResult {
    let lists = new_lists();
    let (both, mine) = (key("absorb", 1), key("absorb", 2));
    let mut request = lists.request(lists.applied_ts());
    let _both_list: &mut ultragraphdb_store_posting::List = request.get(&both)?;
    request.get(&mine)?.add_posting(set_uid(1), false);
    let mut fork = request.fork();
    fork.get(&both)?.add_posting(set_uid(2), false);
    fork.get(&mine)?.add_posting(set_uid(3), false);
    fork.advance_epoch_on_commit();
    fork.remove_rebuild_record_on_commit(4);
    fork.add_entry_at_version(Entry::deletion(key("absorb", 3), 1));
    fork.add_versionless(b"term".to_vec(), b"uid".to_vec());
    request.absorb(fork);
    let read_ts = request.read_ts();
    // Only the fork wrote `both`, so its list is taken; both wrote `mine`, so the request's own stays.
    assert_eq!(uids(request.get(&both)?, read_ts, 0)?, rids(&[2]));
    assert_eq!(uids(request.get(&mine)?, read_ts, 0)?, rids(&[1]));
    assert_eq!(
        request.entry_keys().collect::<Vec<_>>(),
        vec![key("absorb", 3).as_slice()]
    );
    assert_eq!(request.versionless().len(), 1);
    let epoch = lists.epoch();
    let mut batch = lists.batch();
    batch.push(request);
    let _versions: Vec<u64> = lists.commit(batch)?;
    assert_eq!(lists.epoch(), epoch + 1, "the fork's epoch mark is taken");
    Ok(())
}

#[test]
fn test_a_requests_entry_keys_are_its_entries_in_order() -> TestResult {
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
    Ok(())
}

#[test]
fn test_a_request_holds_its_read_open_until_it_is_dropped() -> TestResult {
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
    Ok(())
}

#[test]
fn test_a_list_whose_writes_cancelled_out_is_not_written() -> TestResult {
    let lists = new_lists();
    let k = key("cancelled", 1);
    let mut request = lists.request(lists.applied_ts());
    let list = request.get_from_delta(&k);
    list.add_posting(set_uid(7), true);
    list.add_posting(del_uid(7), true);
    assert_eq!(list.delta().map(|d| d.postings().len()), Some(0));
    assert!(request.deltas().is_empty());
    assert_eq!(request.written_keys().count(), 0);
    Ok(())
}

#[test]
fn test_absorbing_keeps_an_epoch_mark_that_both_carry() -> TestResult {
    let lists = new_lists();
    let epoch = lists.epoch();
    let mut request = lists.request(lists.applied_ts());
    request.advance_epoch_on_commit();
    let mut fork = request.fork();
    fork.advance_epoch_on_commit();
    request.absorb(fork);
    let mut batch = lists.batch();
    batch.push(request);
    let _versions: Vec<u64> = lists.commit(batch)?;
    assert_eq!(lists.epoch(), epoch + 1);
    Ok(())
}

/// A tested statement is recorded by its list key and object, and the list it read is not a read key: an insert's
/// existence check conflicts only with a commit changing that object.
#[test]
fn test_a_tested_statement_is_recorded_and_its_list_is_not_read_whole() -> TestResult {
    let lists = new_lists();
    let k = key("knows", 1);
    commit(&lists, &k, &[set_uid(2)])?;
    let read_ts = lists.applied_ts();
    let mut request = lists.request(read_ts);
    assert_eq!(
        uids(request.get_tested(&k, b"o3")?, read_ts, 0)?,
        rids(&[2])
    );
    assert_eq!(
        request.tested().collect::<Vec<_>>(),
        vec![(k.as_slice(), b"o3".as_slice())]
    );
    assert_eq!(request.read_keys().count(), 0);
    Ok(())
}

/// A list tested and then read whole is a read key, and its test stays recorded.
#[test]
fn test_a_list_read_whole_after_a_test_is_a_read_key() -> TestResult {
    let lists = new_lists();
    let k = key("knows", 2);
    commit(&lists, &k, &[set_uid(2)])?;
    let mut request = lists.request(lists.applied_ts());
    let _k_tested: &mut ultragraphdb_store_posting::List = request.get_tested(&k, b"o")?;
    let _k_list: &mut ultragraphdb_store_posting::List = request.get(&k)?;
    assert_eq!(request.read_keys().collect::<Vec<_>>(), vec![k.as_slice()]);
    assert_eq!(request.tested().count(), 1);
    Ok(())
}

/// A test after a whole read leaves the list read whole.
#[test]
fn test_a_test_after_a_whole_read_keeps_the_list_read_whole() -> TestResult {
    let lists = new_lists();
    let k = key("knows", 3);
    let mut request = lists.request(lists.applied_ts());
    let _k_list: &mut ultragraphdb_store_posting::List = request.get(&k)?;
    let _k_tested: &mut ultragraphdb_store_posting::List = request.get_tested(&k, b"o")?;
    assert_eq!(request.read_keys().collect::<Vec<_>>(), vec![k.as_slice()]);
    Ok(())
}

/// A test of a list written blind completes it from the store, its writes kept on top, and still reads no list
/// whole.
#[test]
fn test_a_test_completes_a_list_written_blind() -> TestResult {
    let lists = new_lists();
    let k = key("knows", 4);
    commit(&lists, &k, &[set_uid(2)])?;
    lists.cache().remove(&k);
    let read_ts = lists.applied_ts();
    let mut request = lists.request(read_ts);
    request.get_from_delta(&k).add_posting(set_uid(3), false);
    assert_eq!(
        uids(request.get_tested(&k, b"o")?, read_ts, 0)?,
        rids(&[2, 3])
    );
    assert_eq!(request.read_keys().count(), 0);
    assert_eq!(
        request.written_keys().collect::<Vec<_>>(),
        vec![k.as_slice()]
    );
    Ok(())
}

/// Each statement is recorded once, in key order then object order, whatever order and however often it was tested.
#[test]
fn test_tested_statements_are_recorded_once_in_order() -> TestResult {
    let lists = new_lists();
    let (low, high) = (key("knows", 5), key("knows", 6));
    assert!(low < high, "the keys sort by subject");
    let mut request = lists.request(lists.applied_ts());
    let _high_b: &mut ultragraphdb_store_posting::List = request.get_tested(&high, b"b")?;
    let _low_z: &mut ultragraphdb_store_posting::List = request.get_tested(&low, b"z")?;
    let _high_a: &mut ultragraphdb_store_posting::List = request.get_tested(&high, b"a")?;
    let _high_b_again: &mut ultragraphdb_store_posting::List = request.get_tested(&high, b"b")?;
    assert_eq!(
        request.tested().collect::<Vec<_>>(),
        vec![
            (low.as_slice(), b"z".as_slice()),
            (high.as_slice(), b"a".as_slice()),
            (high.as_slice(), b"b".as_slice()),
        ]
    );
    Ok(())
}

/// A scan returns the keys of the range the store holds at the request's read timestamp and those the request wrote,
/// in key order and once each, and records the range.
#[test]
fn test_a_scan_returns_stored_and_written_keys_of_its_range_and_records_it() -> TestResult {
    let lists = new_lists();
    let keys: Vec<Vec<u8>> = (1..=5).map(|s| key("scan", s)).collect();
    for k in [&keys[0], &keys[1], &keys[3]] {
        commit(&lists, k, &[set_uid(9)])?;
    }
    let mut request = lists.request(lists.applied_ts());
    request
        .get_from_delta(&keys[2])
        .add_posting(set_uid(1), false);
    request.get(&keys[3])?.add_posting(set_uid(2), false);
    request
        .get_from_delta(&keys[4])
        .add_posting(set_uid(1), false);
    let found = keys_of(&mut request, keys[1].clone(), Some(keys[4].clone()))?;
    // keys[0] lies before the range and keys[4], written, at its end, which is exclusive.
    assert_eq!(
        found,
        vec![keys[1].clone(), keys[2].clone(), keys[3].clone()]
    );
    assert_eq!(
        request.ranges().collect::<Vec<_>>(),
        vec![(keys[1].as_slice(), Some(keys[4].as_slice()))]
    );
    Ok(())
}

/// A scan without an end runs to the last key.
#[test]
fn test_a_scan_without_an_end_runs_to_the_last_key() -> TestResult {
    let lists = new_lists();
    let (a, b) = (key("open", 1), key("open", 2));
    commit(&lists, &a, &[set_uid(1)])?;
    commit(&lists, &b, &[set_uid(1)])?;
    let mut request = lists.request(lists.applied_ts());
    assert_eq!(keys_of(&mut request, b.clone(), None)?, vec![b.clone()]);
    assert_eq!(
        request.ranges().collect::<Vec<_>>(),
        vec![(b.as_slice(), None)]
    );
    Ok(())
}

/// An empty range finds nothing and is recorded all the same: a commit can still write a key there only if the range
/// holds one, so it conflicts with nothing, and the record costs nothing.
#[test]
fn test_an_empty_range_finds_nothing_and_is_recorded() -> TestResult {
    let lists = new_lists();
    let a = key("empty", 1);
    commit(&lists, &a, &[set_uid(1)])?;
    let mut request = lists.request(lists.applied_ts());
    assert!(keys_of(&mut request, a.clone(), Some(a.clone()))?.is_empty());
    assert_eq!(request.ranges().count(), 1);
    Ok(())
}

/// A scan reads the store at the request's read timestamp: a key committed after it is not found.
#[test]
fn test_a_scan_reads_the_store_at_the_read_timestamp() -> TestResult {
    let lists = new_lists();
    let (a, b) = (key("later", 1), key("later", 2));
    commit(&lists, &a, &[set_uid(1)])?;
    let mut request = lists.request(lists.applied_ts());
    commit(&lists, &b, &[set_uid(1)])?;
    let found = keys_of(&mut request, key("later", 0), None)?;
    assert_eq!(found, vec![a]);
    Ok(())
}

/// A list the request emptied is still a key of the store at its read timestamp; one it wrote and cancelled out, and
/// that the store does not hold, is no key at all.
#[test]
fn test_a_list_written_and_cancelled_out_is_not_found() -> TestResult {
    let lists = new_lists();
    let (held, cancelled) = (key("cancel", 1), key("cancel", 2));
    commit(&lists, &held, &[set_uid(1)])?;
    let mut request = lists.request(lists.applied_ts());
    request.get(&held)?.add_posting(del_uid(1), false);
    let list = request.get_from_delta(&cancelled);
    list.add_posting(set_uid(7), true);
    list.add_posting(del_uid(7), true);
    assert_eq!(keys_of(&mut request, key("cancel", 0), None)?, vec![held]);
    Ok(())
}

/// A range the caller records is reported with the request's scans, each range once; a range another reaches extends that
/// one, and a range past it stands apart.
#[test]
fn test_a_recorded_range_merges_with_the_one_reaching_it() -> TestResult {
    let lists = new_lists();
    let mut request = lists.request(lists.applied_ts());
    let (a, b, c, d, e) = (
        b"\x10a".to_vec(),
        b"\x10b".to_vec(),
        b"\x10c".to_vec(),
        b"\x10d".to_vec(),
        b"\x10e".to_vec(),
    );
    request.record_range(a.clone(), Some(b.clone()));
    request.record_range(a.clone(), Some(b.clone()));
    assert_eq!(request.range_count(), 1, "one range, recorded twice");
    request.record_range(b.clone(), Some(c.clone()));
    request.record_range(d.clone(), Some(e.clone()));
    assert_eq!(
        request.ranges().collect::<Vec<_>>(),
        vec![
            (a.as_slice(), Some(c.as_slice())),
            (d.as_slice(), Some(e.as_slice())),
        ]
    );
    assert_eq!(request.range_bytes(), a.len() + c.len() + d.len() + e.len());
    request.record_range(c.clone(), None);
    assert_eq!(
        request.ranges().collect::<Vec<_>>(),
        vec![(a.as_slice(), None), (d.as_slice(), Some(e.as_slice())),],
        "a range reaching the last key takes the one it reaches into only by its own start"
    );
    Ok(())
}

/// The keys `request` walks in the range, every one a row.
fn keys_of<S, C>(
    request: &mut ultragraphdb_store_posting::Request<'_, S, C>,
    start: Vec<u8>,
    end: Option<Vec<u8>>,
) -> Result<Vec<Vec<u8>>, ultragraphdb_store_posting::PostingError>
where
    S: ultragraphdb_traits_store::Store,
    C: ListCache<Value = ultragraphdb_store_posting::CachedList>,
{
    let mut keys = Vec::new();
    let _exhausted = request.span(start, end, WalkBounds::default(), |key, _| {
        keys.push(key.to_vec());
        Ok::<_, ultragraphdb_store_posting::PostingError>(WalkStep::Row)
    })?;
    Ok(keys)
}

/// A span at a limit gives that many rows, covers what it returned and no more, and the next from where it ended extends the
/// one range; nothing is held and no list is read whole, and a list the request emptied gives no row.
#[test]
fn test_a_span_covers_what_it_returned_and_holds_nothing() -> TestResult {
    let lists = new_lists();
    let keys: Vec<Vec<u8>> = (1..=4).map(|n| key("span", n)).collect();
    for k in &keys {
        commit(&lists, k, &[set_uid(1)])?;
    }
    let mut request = lists.request(lists.applied_ts());
    let last = keys.last().context("a last key")?.clone();
    let first = keys.first().context("a first key")?.clone();
    let limit = std::num::NonZeroUsize::MIN;
    let mut seen = Vec::new();
    let mut from = first.clone();
    let mut spans = 0;
    loop {
        spans += 1;
        let end = request.span(
            from.clone(),
            Some(last.clone()),
            WalkBounds::rows(Some(limit)),
            |key, _| {
                seen.push(key.to_vec());
                Ok::<_, ultragraphdb_store_posting::PostingError>(WalkStep::Row)
            },
        )?;
        let Some(resume) = end.past_last() else {
            break;
        };
        let mut covered = seen.last().context("a row")?.clone();
        covered.push(0);
        assert_eq!(resume, covered, "the span ends just past its last row");
        assert_eq!(
            request.ranges().collect::<Vec<_>>(),
            vec![(first.as_slice(), Some(covered.as_slice()))],
            "what it returned, in one range"
        );
        from = resume;
    }
    assert_eq!(seen, keys[..3].to_vec());
    assert!(spans > 1, "the limit stopped it");
    assert_eq!(
        request.ranges().collect::<Vec<_>>(),
        vec![(first.as_slice(), Some(last.as_slice()))],
        "the range ended, so it covers through its past key"
    );
    assert_eq!(request.held_count(), 0);
    assert_eq!(request.read_keys().count(), 0);
    Ok(())
}

/// A span paused records what it gave and starts the next where it ended, so the frames of one scan hold one range: inside a
/// key through that key, between keys through the last key, and nothing when it gave nothing.
#[test]
fn test_a_paused_span_records_through_its_last_key_and_nothing_when_it_gave_nothing() -> TestResult
{
    let lists = new_lists();
    let keys: Vec<Vec<u8>> = (1..=4).map(|n| key("paused", n)).collect();
    for k in &keys {
        commit(&lists, k, &[set_uid(1)])?;
    }
    let (first, second, third, end) = (
        keys.first().context("a key")?.clone(),
        keys.get(1).context("a key")?.clone(),
        keys.get(2).context("a key")?.clone(),
        key("paused", 9),
    );
    let past = |k: &[u8]| [k, &[0]].concat();
    let span = |request: &mut ultragraphdb_store_posting::Request<'_, _, _>,
                from: Vec<u8>,
                step: WalkStep| {
        request.span(from, Some(end.clone()), WalkBounds::default(), |_, _| {
            Ok::<_, ultragraphdb_store_posting::PostingError>(step)
        })
    };
    let mut request = lists.request(lists.applied_ts());
    let nothing = span(&mut request, first.clone(), WalkStep::PausedBefore)?;
    assert_eq!(
        nothing,
        WalkEnd::Paused {
            last: None,
            next: first.clone()
        }
    );
    assert_eq!(request.range_count(), 0, "it gave nothing");

    let inside = span(&mut request, first.clone(), WalkStep::PausedInside)?;
    assert_eq!(
        inside,
        WalkEnd::Paused {
            last: Some(first.clone()),
            next: first.clone()
        }
    );
    let through_first = past(&first);
    assert_eq!(
        request.ranges().collect::<Vec<_>>(),
        vec![(first.as_slice(), Some(through_first.as_slice()))]
    );

    // Resumed inside the key it paused in, then between keys: one range.
    let mut steps = [WalkStep::Row, WalkStep::Row, WalkStep::PausedBefore].into_iter();
    let bounds = WalkBounds::default();
    let between = request.span(first.clone(), Some(end.clone()), bounds, |_, _| {
        Ok::<_, ultragraphdb_store_posting::PostingError>(steps.next().unwrap_or(WalkStep::Row))
    })?;
    assert_eq!(
        between,
        WalkEnd::Paused {
            last: Some(second.clone()),
            next: third
        }
    );
    let through_second = past(&second);
    assert_eq!(
        request.ranges().collect::<Vec<_>>(),
        vec![(first.as_slice(), Some(through_second.as_slice()))]
    );

    let last = span(&mut request, through_second.clone(), WalkStep::Row)?;
    assert_eq!(last, WalkEnd::Exhausted);
    assert_eq!(
        request.ranges().collect::<Vec<_>>(),
        vec![(first.as_slice(), Some(end.as_slice()))],
        "the range ended, so it covers through its past key"
    );
    Ok(())
}

/// A span reads a list as the request sees it, changing nothing: a held list with its writes, a list written blind with the
/// store beneath its delta, and any other from the store.
#[test]
fn test_a_span_reads_the_requests_writes_over_the_store() -> TestResult {
    let lists = new_lists();
    let (held, blind, other) = (key("over", 1), key("over", 2), key("over", 3));
    let (fresh, stored) = (key("over", 4), 1);
    for k in [&held, &blind, &other] {
        commit(&lists, k, &[set_uid(stored)])?;
    }
    let mut request = lists.request(lists.applied_ts());
    let ts = request.read_ts();
    request.get(&held)?.add_posting(del_uid(stored), false);
    request
        .get_from_delta(&blind)
        .add_posting(set_uid(2), false);
    request
        .get_from_delta(&fresh)
        .add_posting(set_uid(3), false);
    let held_before = request.held_count();
    let (reads_before, mut rows) = (request.read_keys().count(), Vec::new());
    let _exhausted = request.span(key("over", 0), None, WalkBounds::default(), |k, source| {
        let uids = source.with_list(k, ts, |list| uids_of(list, ts))??;
        rows.push((k.to_vec(), uids));
        Ok::<_, ultragraphdb_store_posting::PostingError>(WalkStep::Row)
    })?;
    let mut expected = vec![
        (held, vec![]),
        (blind, vec![set_uid(stored).uid(), set_uid(2).uid()]),
        (other, vec![set_uid(stored).uid()]),
        (fresh, vec![set_uid(3).uid()]),
    ];
    expected.sort();
    assert_eq!(rows, expected, "in key order");
    assert_eq!(request.held_count(), held_before, "nothing newly held");
    assert_eq!(
        request.read_keys().count(),
        reads_before,
        "nothing newly read whole"
    );
    Ok(())
}

/// The uids `list` holds at `ts`.
fn uids_of(
    list: &ultragraphdb_store_posting::List,
    ts: u64,
) -> Result<Vec<u128>, ultragraphdb_store_posting::PostingError> {
    let mut uids = Vec::new();
    list.iterate(ts, 0, |posting| {
        uids.push(posting.uid());
        Ok::<_, ultragraphdb_store_posting::PostingError>(std::ops::ControlFlow::Continue(()))
    })?;
    Ok(uids)
}

/// Absorbing a fork takes its tested statements and ranges, and a list either read whole stays read whole.
#[test]
fn test_absorbing_a_fork_keeps_its_tests_ranges_and_whole_reads() -> TestResult {
    let lists = new_lists();
    let (tested, scanned) = (key("absorbed", 1), key("absorbed", 2));
    let mut request = lists.request(lists.applied_ts());
    let _tested_mine: &mut ultragraphdb_store_posting::List =
        request.get_tested(&tested, b"mine")?;
    request.get(&scanned)?.add_posting(set_uid(1), false);
    let mut fork = request.fork();
    let _tested_list: &mut ultragraphdb_store_posting::List = fork.get(&tested)?;
    let _tested_theirs: &mut ultragraphdb_store_posting::List =
        fork.get_tested(&tested, b"theirs")?;
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
    Ok(())
}

/// Seeded deltas are the request's own writes: a read completes the list from the store and keeps them on top, a later
/// edge adds to them, and the request leaves one delta per list, the seeded postings with its own (7.5).
#[test]
fn test_seeded_deltas_are_the_requests_own_writes() -> TestResult {
    let lists = new_lists();
    let hub = key("hub", 1);
    let other = key("other", 1);
    commit(&lists, &hub, &[set_uid(1)])?;
    let read_ts = lists.applied_ts();
    let mut first = lists.request(read_ts);
    first.get(&hub)?.add_posting(set_uid(2).clone(), false);
    first.get(&other)?.add_posting(set_uid(7).clone(), false);
    let writes = first.into_writes();

    let mut request = lists.request(read_ts);
    request.seed_deltas(writes.deltas());
    check_uids(request.get(&hub)?, &[1, 2], read_ts)?;
    request.get(&hub)?.add_posting(set_uid(3).clone(), false);
    let combined = request.into_writes();
    let deltas: Vec<(Vec<u8>, Vec<u128>)> = combined
        .deltas()
        .iter()
        .map(|(k, d)| (k.clone(), d.postings().iter().map(|p| p.uid()).collect()))
        .collect();
    assert_eq!(
        deltas,
        vec![(hub.clone(), rids(&[2, 3])), (other.clone(), rids(&[7]))],
        "one delta per list, the seeded postings and the request's"
    );
    Ok(())
}

/// A versionless entry added as it is keeps its removal.
#[test]
fn test_a_versionless_entry_is_added_as_it_is() -> TestResult {
    let lists = new_lists();
    let mut request = lists.request(lists.applied_ts());
    request.add_versionless_entry(ultragraphdb_traits_store::VersionlessEntry::removal(
        b"k".to_vec(),
    ));
    request.add_versionless(b"j".to_vec(), b"v".to_vec());
    let writes = request.into_writes();
    assert!(writes.versionless()[0].is_removal());
    assert_eq!(writes.versionless()[1].value(), Some(&b"v"[..]));
    Ok(())
}

/// A snapshot is the list as the request's read timestamp holds it, the batch's earlier writes beneath, without the
/// request's own writes, whether it read the list or wrote it blind, and it records no read.
#[test]
fn test_a_snapshot_leaves_out_the_requests_own_writes_and_records_no_read() -> TestResult {
    let lists = new_lists();
    let (read, blind, stacked) = (key("read", 1), key("blind", 1), key("stacked", 1));
    commit(&lists, &read, &[set_uid(1)])?;
    commit(&lists, &blind, &[set_uid(2)])?;
    let read_ts = lists.applied_ts();
    let mut batch = lists.batch();
    let mut earlier = batch.request(read_ts);
    earlier
        .get_from_delta(&stacked)
        .add_posting(set_uid(5).clone(), false);
    batch.push(earlier);
    let mut request = batch.request(read_ts);
    request.get(&read)?.add_posting(set_uid(3).clone(), false);
    request
        .get_from_delta(&blind)
        .add_posting(set_uid(4).clone(), false);
    request
        .get_from_delta(&stacked)
        .add_posting(set_uid(6).clone(), false);
    check_uids(&request.snapshot(&read)?, &[1], read_ts)?;
    check_uids(&request.snapshot(&blind)?, &[2], read_ts)?;
    check_uids(&request.snapshot(&stacked)?, &[5], read_ts)?;
    check_uids(&request.snapshot(&key("unwritten", 1))?, &[], read_ts)?;
    assert_eq!(
        request.read_keys().collect::<Vec<_>>(),
        vec![read.as_slice()],
        "a snapshot is no read"
    );
    check_uids(request.get(&read)?, &[1, 3], read_ts)?;
    Ok(())
}

/// Discarding the deltas keeps every read the request recorded and the lists it read, which read the snapshot again;
/// a list written blind is dropped, and later writes are the request's only deltas.
#[test]
fn test_discarding_the_deltas_keeps_the_reads_and_drops_the_writes() -> TestResult {
    let lists = new_lists();
    let (read, tested, blind) = (key("read", 1), key("tested", 1), key("blind", 1));
    commit(&lists, &read, &[set_uid(1)])?;
    let read_ts = lists.applied_ts();
    let mut request = lists.request(read_ts);
    request.get(&read)?.add_posting(set_uid(2).clone(), false);
    request
        .get_tested(&tested, b"o")?
        .add_posting(set_uid(3).clone(), false);
    request
        .get_from_delta(&blind)
        .add_posting(set_uid(4).clone(), false);
    request.discard_deltas();
    assert!(request.deltas().is_empty(), "no delta is left");
    assert_eq!(
        request.read_keys().collect::<Vec<_>>(),
        vec![read.as_slice()]
    );
    assert_eq!(
        request.tested().collect::<Vec<_>>(),
        vec![(tested.as_slice(), &b"o"[..])]
    );
    assert_eq!(request.lists().count(), 2, "the blind list is dropped");
    check_uids(request.get(&read)?, &[1], read_ts)?;
    request.get(&read)?.add_posting(set_uid(7).clone(), false);
    let writes = request.into_writes();
    let deltas: Vec<(Vec<u8>, Vec<u128>)> = writes
        .deltas()
        .iter()
        .map(|(k, d)| (k.clone(), d.postings().iter().map(|p| p.uid()).collect()))
        .collect();
    assert_eq!(deltas, vec![(read, rids(&[7]))]);
    Ok(())
}

/// A span handing back at its bound of keys giving nothing covers them, so a later write into them conflicts, and the span going
/// on from there extends it.
#[test]
fn test_a_yielded_span_covers_the_empty_keys_it_read() -> TestResult {
    let lists = new_lists();
    let keys: Vec<Vec<u8>> = (1..=3).map(|n| key("yielded", n)).collect();
    for k in &keys {
        commit(&lists, k, &[set_uid(1)])?;
    }
    let mut request = lists.request(lists.applied_ts());
    let first = keys.first().context("a first key")?.clone();
    let second = keys.get(1).context("a second key")?.clone();
    let end = keys.last().context("a last key")?.clone();
    let bounds = WalkBounds::rows(None).with_idle(Some(std::num::NonZeroUsize::MIN));
    let walked = request.span(first.clone(), Some(end.clone()), bounds, |_, _| {
        Ok::<_, ultragraphdb_store_posting::PostingError>(WalkStep::Nothing)
    })?;
    let next = [first.as_slice(), &[0]].concat();
    assert_eq!(walked, WalkEnd::Yielded { next: next.clone() });
    assert_eq!(
        request.ranges().collect::<Vec<_>>(),
        vec![(first.as_slice(), Some(next.as_slice()))]
    );
    let on = request.span(next, Some(second.clone()), WalkBounds::default(), |_, _| {
        Ok::<_, ultragraphdb_store_posting::PostingError>(WalkStep::Nothing)
    })?;
    assert_eq!(on, WalkEnd::Exhausted);
    assert_eq!(
        request.ranges().collect::<Vec<_>>(),
        vec![(first.as_slice(), Some(second.as_slice()))],
        "one range"
    );
    Ok(())
}
