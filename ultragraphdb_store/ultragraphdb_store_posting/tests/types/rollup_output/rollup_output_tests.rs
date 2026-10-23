/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::encode;
use ultragraphdb_proto_pb::pb::PostingList;
use ultragraphdb_store_posting::{BLOCK_SIZE, List, should_split};

use crate::utils::helpers::{add, del_uid, key, set_uid};

#[test]
fn test_a_list_splits_only_over_more_than_one_block() {
    let one_block = PostingList {
        pack: encode(&(1..=200).collect::<Vec<_>>(), BLOCK_SIZE),
        ..PostingList::default()
    };
    assert!(!should_split(&one_block, 1));
    let two_blocks = PostingList {
        pack: encode(&(1..=300).collect::<Vec<_>>(), BLOCK_SIZE),
        ..PostingList::default()
    };
    assert!(should_split(&two_blocks, 1));
    assert!(!should_split(&two_blocks, usize::MAX));
}

#[test]
fn test_a_list_left_with_one_empty_part_becomes_unsplit() {
    let mut l = List::empty(key("unsplit", 1));
    (1..=1000).for_each(|u| add(&mut l, 1, &set_uid(u)));
    l.commit_mutation(2);
    let out = l.rollup(3, true, 1000).unwrap().unwrap();
    assert!(out.plist().splits.len() > 1);
    assert_eq!(out.splits(), out.plist().splits);
    assert_eq!(out.splits()[0], 1);

    let split = List::new(key("unsplit", 1), out.plist().clone(), 2)
        .with_parts(out.parts().values().cloned().collect());
    let mut emptied = split;
    (1..=1000).for_each(|u| add(&mut emptied, 3, &del_uid(u)));
    emptied.commit_mutation(4);
    let out = emptied.rollup(5, true, 1000).unwrap().unwrap();
    assert!(out.parts().is_empty());
    assert!(out.plist().splits.is_empty());
    assert_eq!(out.new_min_ts(), 4);
}
