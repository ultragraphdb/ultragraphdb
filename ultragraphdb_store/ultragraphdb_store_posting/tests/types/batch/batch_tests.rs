/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::utils::helpers::{check_uids, commit, key, new_lists, set_uid, set_value};
use ultragraphdb_tool_bdd::{Context, TestResult};

#[test]
fn test_a_batch_holds_its_requests_in_order() -> TestResult {
    let lists = new_lists();
    let mut batch = lists.batch();
    assert!(batch.is_empty());
    for uid in 1..=3 {
        let mut request = batch.request(lists.applied_ts());
        request
            .get(&key("batch", uid))?
            .add_posting(set_uid(uid).clone(), false);
        batch.push(request);
    }
    assert_eq!(batch.len(), 3);
    assert!(!batch.is_empty());
    let keys: Vec<Vec<u8>> = batch
        .requests()
        .iter()
        .map(|r| r.deltas()[0].0.clone())
        .collect();
    assert_eq!(keys, (1..=3).map(|u| key("batch", u)).collect::<Vec<_>>());
    Ok(())
}

/// A later request's static value, the read scalar values and the vector index take, is the newest write of the
/// requests before it in its batch, above the committed versions; a read at another timestamp does not see it.
#[test]
fn test_a_later_request_reads_an_earlier_ones_static_value() -> TestResult {
    let lists = new_lists();
    let name = key("name", 1);
    let mut first = lists.request(lists.applied_ts());
    first.get(&name)?.add_posting(set_value("A").clone(), false);
    let mut committed = lists.batch();
    committed.push(first);
    let _versions: Vec<u64> = lists.commit(committed)?;

    let mut batch = lists.batch();
    let read_ts = lists.applied_ts();
    let mut earlier = batch.request(read_ts);
    earlier
        .get(&name)?
        .add_posting(set_value("B").clone(), false);
    batch.push(earlier);
    let mut later = batch.request(read_ts);
    let list = later.get(&name)?;
    assert_eq!(
        list.mutation().len(),
        2,
        "one committed delta and one stacked layer"
    );
    assert_eq!(
        list.static_value(read_ts)
            .context("a static value")?
            .postings()[0]
            .value(),
        b"B"
    );
    assert_eq!(
        list.static_value(read_ts + 1)
            .context("a static value")?
            .postings()[0]
            .value(),
        b"A"
    );
    Ok(())
}

/// Begins `later` requests in one batch after a first one that writes key `hub`, each later one writing a key of its
/// own and pushed, then reads `hub` in one more request: the holders of the first request's stacked layer.
fn holders_of_the_first_layer(later: u64) -> TestResult<usize> {
    let lists = new_lists();
    let read_ts = lists.applied_ts();
    let hub = key("hub", 1);
    let mut batch = lists.batch();
    let mut first = batch.request(read_ts);
    first.get(&hub)?.add_posting(set_uid(1).clone(), false);
    batch.push(first);
    for uid in 2..2 + later {
        let mut request = batch.request(read_ts);
        request
            .get(&key("node", uid))?
            .add_posting(set_uid(uid).clone(), false);
        batch.push(request);
    }
    let mut reader = batch.request(read_ts);
    let list = reader.get(&hub)?;
    assert_eq!(
        list.mutation().stacked().len(),
        1,
        "the first request's write"
    );
    Ok(std::sync::Arc::strong_count(&list.mutation().stacked()[0]))
}

/// A request begun from a batch shares the layers its batch has stacked rather than copying the batch's map of them:
/// how many hold one layer does not grow with the requests begun after it, so a batch of n requests costs O(n) map
/// entries, not O(n²).
#[test]
fn test_a_request_shares_the_batchs_stacked_layers_whatever_the_batch_size() -> TestResult {
    let few = holders_of_the_first_layer(10)?;
    let many = holders_of_the_first_layer(1_000)?;
    assert_eq!(
        few, many,
        "holders grew with the batch: {few} against {many}"
    );
    // The batch's stack and the list read.
    assert!(many <= 2, "{many} holders");
    Ok(())
}

/// A request sees the writes of the requests pushed before it began, not of those pushed after.
#[test]
fn test_a_request_sees_the_layers_pushed_before_it_began_and_none_after() -> TestResult {
    let lists = new_lists();
    let read_ts = lists.applied_ts();
    let hub = key("hub", 1);
    let mut batch = lists.batch();
    let mut first = batch.request(read_ts);
    first.get(&hub)?.add_posting(set_uid(1).clone(), false);
    let mut early = batch.request(read_ts);
    batch.push(first);
    let mut late = batch.request(read_ts);
    assert!(early.get(&hub)?.mutation().stacked().is_empty());
    assert_eq!(late.get(&hub)?.mutation().stacked().len(), 1);
    Ok(())
}

/// Writes stacked in a batch are read by every request begun after them, above the committed state, and the batch
/// commits nothing of them: the commit step's overlay of decided but unapplied rounds (7.5).
#[test]
fn test_stacked_writes_are_read_by_later_requests_and_never_committed() -> TestResult {
    let lists = new_lists();
    let hub = key("hub", 1);
    commit(&lists, &hub, &[set_uid(1)])?;
    let read_ts = lists.applied_ts();
    let mut written = lists.request(read_ts);
    written.get(&hub)?.add_posting(set_uid(2).clone(), false);
    let writes = written.into_writes();
    let mut batch = lists.batch();
    let before = batch.request(read_ts);
    batch.stack(&writes);
    batch.stack(&writes);
    let mut after = batch.request(read_ts);
    check_uids(after.get(&hub)?, &[1, 2], read_ts)?;
    let mut earlier = before;
    check_uids(earlier.get(&hub)?, &[1], read_ts)?;
    assert!(
        batch.is_empty(),
        "a stacked write is no request of the batch"
    );
    let mut fresh = lists.request(read_ts);
    check_uids(fresh.get(&hub)?, &[1], read_ts)?;
    Ok(())
}
