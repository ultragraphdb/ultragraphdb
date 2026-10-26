/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;

use prost::Message;

use ultragraphdb_core_codec::encode;
use ultragraphdb_proto_pb::pb::PostingList;
use ultragraphdb_store_posting::{BLOCK_SIZE, CachedList, List};

use crate::utils::helpers::{add, key, set_uid};

#[test]
fn test_the_weigher_counts_the_lists_bytes() {
    let k = key("weigh", 1);
    let empty = CachedList::new(Arc::new(List::empty(k.clone())), 1, 0);
    let small = empty.weight();
    // The fixed overhead of 64 and the key, 33 bytes: the keyspace byte and four u64s.
    assert_eq!(k.len(), 33);
    assert_eq!(small, 64 + 33);

    let uids: Vec<u64> = (1..=10_000).collect();
    let complete = PostingList {
        pack: encode(&uids, BLOCK_SIZE),
        ..PostingList::default()
    };
    let big = CachedList::new(Arc::new(List::new(k.clone(), complete.clone(), 1)), 1, 0);
    assert_eq!(
        big.weight(),
        small + complete.encoded_len() as u32,
        "the pack is weighed"
    );
    assert!(complete.encoded_len() > 10_000);

    let mut with_delta = List::new(k, complete, 1);
    add(&mut with_delta, 2, &set_uid(20_000));
    with_delta.commit_mutation(3);
    let deltas = CachedList::new(Arc::new(with_delta), 3, 0);
    let delta_len: usize = deltas
        .list()
        .mutation()
        .committed()
        .values()
        .map(|d| d.encoded_len())
        .sum();
    assert!(delta_len > 0);
    assert_eq!(
        deltas.weight(),
        big.weight() + delta_len as u32,
        "the deltas are weighed"
    );
    assert_eq!((deltas.ts(), deltas.epoch()), (3, 0));
}
