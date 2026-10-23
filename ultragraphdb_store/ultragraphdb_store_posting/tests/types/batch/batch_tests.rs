/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::utils::helpers::{key, new_lists, set_uid};

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
