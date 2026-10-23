/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;

use ultragraphdb_core_codec::encode;
use ultragraphdb_proto_pb::pb::PostingList;
use ultragraphdb_store_posting::{BLOCK_SIZE, CachedList, List};

use crate::utils::helpers::{add, key, set_uid};

#[test]
fn test_the_weigher_counts_the_lists_bytes() {
    let k = key("weigh", 1);
    let empty = CachedList::new(Arc::new(List::empty(k.clone())), 1, 0);
    let small = empty.weight();
    assert!(small as usize >= k.len());

    let uids: Vec<u64> = (1..=10_000).collect();
    let complete = PostingList {
        pack: encode(&uids, BLOCK_SIZE),
        ..PostingList::default()
    };
    let big = CachedList::new(Arc::new(List::new(k.clone(), complete.clone(), 1)), 1, 0);
    assert!(big.weight() > small + 10_000, "the pack is weighed");

    let mut with_delta = List::new(k, complete, 1);
    add(&mut with_delta, 2, &set_uid(20_000));
    with_delta.commit_mutation(3);
    let deltas = CachedList::new(Arc::new(with_delta), 3, 0);
    assert!(deltas.weight() > big.weight(), "the deltas are weighed");
    assert_eq!((deltas.ts(), deltas.epoch()), (3, 0));
}
