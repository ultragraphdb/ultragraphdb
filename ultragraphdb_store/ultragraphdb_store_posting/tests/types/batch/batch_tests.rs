/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::utils::helpers::{key, new_lists, set_uid, set_value};

#[test]
fn test_a_batch_holds_its_requests_in_order() {
    let lists = new_lists();
    let mut batch = lists.batch();
    assert!(batch.is_empty());
    for uid in 1..=3 {
        let mut request = batch.request(lists.latest_ts());
        request
            .get(&key("batch", uid))
            .unwrap()
            .add_posting(set_uid(uid).clone(), false);
        batch.push(request);
    }
    assert_eq!(batch.len(), 3);
    let keys: Vec<Vec<u8>> = batch
        .requests()
        .iter()
        .map(|r| r.deltas()[0].0.clone())
        .collect();
    assert_eq!(keys, (1..=3).map(|u| key("batch", u)).collect::<Vec<_>>());
}

/// A later request's static value, the read scalar values and the vector index take, is the newest write of the
/// requests before it in its batch, above the committed versions; a read at another timestamp does not see it.
#[test]
fn test_a_later_request_reads_an_earlier_ones_static_value() {
    let lists = new_lists();
    let name = key("name", 1);
    let mut first = lists.request(lists.fresh_ts());
    first
        .get(&name)
        .unwrap()
        .add_posting(set_value("A").clone(), false);
    let mut committed = lists.batch();
    committed.push(first);
    lists.commit(committed).unwrap();

    let mut batch = lists.batch();
    let read_ts = lists.fresh_ts();
    let mut earlier = batch.request(read_ts);
    earlier
        .get(&name)
        .unwrap()
        .add_posting(set_value("B").clone(), false);
    batch.push(earlier);
    let mut later = batch.request(read_ts);
    let list = later.get(&name).unwrap();
    assert_eq!(list.static_value(read_ts).unwrap().postings[0].value, b"B");
    assert_eq!(
        list.static_value(read_ts + 1).unwrap().postings[0].value,
        b"A"
    );
}

/// Begins `later` requests in one batch after a first one that writes key `hub`, each later one writing a key of its
/// own and pushed, then reads `hub` in one more request: the holders of the first request's stacked layer.
fn holders_of_the_first_layer(later: u64) -> usize {
    let lists = new_lists();
    let read_ts = lists.fresh_ts();
    let hub = key("hub", 1);
    let mut batch = lists.batch();
    let mut first = batch.request(read_ts);
    first
        .get(&hub)
        .unwrap()
        .add_posting(set_uid(1).clone(), false);
    batch.push(first);
    for uid in 2..2 + later {
        let mut request = batch.request(read_ts);
        request
            .get(&key("node", uid))
            .unwrap()
            .add_posting(set_uid(uid).clone(), false);
        batch.push(request);
    }
    let mut reader = batch.request(read_ts);
    let list = reader.get(&hub).unwrap();
    assert_eq!(
        list.mutation().stacked().len(),
        1,
        "the first request's write"
    );
    std::sync::Arc::strong_count(&list.mutation().stacked()[0])
}

/// A request begun from a batch shares the layers its batch has stacked rather than copying the batch's map of them:
/// how many hold one layer does not grow with the requests begun after it, so a batch of n requests costs O(n) map
/// entries, not O(n²).
#[test]
fn test_a_request_shares_the_batchs_stacked_layers_whatever_the_batch_size() {
    let few = holders_of_the_first_layer(10);
    let many = holders_of_the_first_layer(1_000);
    assert_eq!(
        few, many,
        "holders grew with the batch: {few} against {many}"
    );
    // The batch's stack and the list read.
    assert!(many <= 2, "{many} holders");
}

/// A request sees the writes of the requests pushed before it began, not of those pushed after.
#[test]
fn test_a_request_sees_the_layers_pushed_before_it_began_and_none_after() {
    let lists = new_lists();
    let read_ts = lists.fresh_ts();
    let hub = key("hub", 1);
    let mut batch = lists.batch();
    let mut first = batch.request(read_ts);
    first
        .get(&hub)
        .unwrap()
        .add_posting(set_uid(1).clone(), false);
    let mut early = batch.request(read_ts);
    batch.push(first);
    let mut late = batch.request(read_ts);
    assert!(early.get(&hub).unwrap().mutation().stacked().is_empty());
    assert_eq!(late.get(&hub).unwrap().mutation().stacked().len(), 1);
}
