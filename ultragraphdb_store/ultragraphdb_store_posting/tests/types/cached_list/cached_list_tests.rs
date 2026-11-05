/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;
use ultragraphdb_tool_bdd::TestResult;

use ultragraphdb_core_codec::PostingList;
use ultragraphdb_store_posting::{BLOCK_SIZE, CachedList, List};

use crate::utils::helpers::{add, ids, key, pack_of, set_uid};

#[test]
fn test_the_weigher_counts_the_lists_bytes() -> TestResult {
    let k = key("weigh", 1);
    let empty = CachedList::new(Arc::new(List::empty(k.clone())), 1, 0);
    let small = empty.weight();
    // The fixed overhead of 64, the key, 57 bytes: the keyspace byte, the tenant's 8, the graph's, the
    // predicate's and the subject's 16 each, and the empty list's encoding, 26 bytes: the posting format's version,
    // flags, commit version, posting and split counts and checksum.
    assert_eq!(k.len(), 57);
    assert_eq!(small, 64 + 57 + 26);

    let uids: Vec<u64> = (1..=10_000).collect();
    let complete = PostingList::default().with_pack(pack_of(&ids(&uids)?, BLOCK_SIZE)?);
    let big = CachedList::new(Arc::new(List::new(k.clone(), complete.clone(), 1)), 1, 0);
    assert_eq!(
        big.weight(),
        64 + 57 + complete.encoded_len() as u32,
        "the complete list is weighed in place of the empty one"
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
    Ok(())
}
