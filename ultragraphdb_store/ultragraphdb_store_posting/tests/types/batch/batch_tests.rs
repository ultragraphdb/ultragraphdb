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
            .add_mutation(&set_uid(uid), None)
            .unwrap();
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
        .add_mutation(&set_value("A"), None)
        .unwrap();
    let mut committed = lists.batch();
    committed.push(first);
    lists.commit(committed).unwrap();

    let mut batch = lists.batch();
    let read_ts = lists.fresh_ts();
    let mut earlier = batch.request(read_ts);
    earlier
        .get(&name)
        .unwrap()
        .add_mutation(&set_value("B"), None)
        .unwrap();
    batch.push(earlier);
    let mut later = batch.request(read_ts);
    let list = later.get(&name).unwrap();
    assert_eq!(list.static_value(read_ts).unwrap().postings[0].value, b"B");
    assert_eq!(
        list.static_value(read_ts + 1).unwrap().postings[0].value,
        b"A"
    );
}
