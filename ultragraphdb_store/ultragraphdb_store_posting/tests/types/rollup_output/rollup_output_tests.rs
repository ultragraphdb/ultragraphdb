/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::{Posting, PostingList, decode};
use ultragraphdb_store_posting::{
    BIT_COMPLETE_POSTING, BLOCK_SIZE, List, PostingErrorEnum, bin_split, should_split, split_key_of,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::helpers::{Failure, add, del_uid, ids, key, rid, rids, set_uid};

#[test]
fn test_a_list_splits_only_over_more_than_one_block() -> TestResult {
    let one_block = pack_of(200)?;
    assert!(!should_split(&one_block, 1));
    let two_blocks = pack_of(300)?;
    assert!(should_split(&two_blocks, 1));
    assert!(!should_split(&two_blocks, usize::MAX));
    Ok(())
}

#[test]
fn test_a_list_left_with_one_empty_part_becomes_unsplit() -> TestResult {
    let mut l = List::empty(key("unsplit", 1));
    (1..=1000).for_each(|u| add(&mut l, 1, &set_uid(u)));
    l.commit_mutation(2);
    let out = l.rollup(3, true, 1000)?.context("a rollup")?;
    assert!(out.plist().splits().len() > 1);
    assert_eq!(out.splits(), out.plist().splits());
    assert_eq!(
        out.splits()[0],
        0,
        "a forward list's first part starts at 0"
    );

    let split = List::new(key("unsplit", 1), out.plist().clone(), 2)
        .with_parts(out.parts().values().cloned().collect());
    let mut emptied = split;
    (1..=1000).for_each(|u| add(&mut emptied, 3, &del_uid(u)));
    emptied.commit_mutation(4);
    let emptied_out = emptied.rollup(5, true, 1000)?.context("a rollup")?;
    assert!(emptied_out.parts().is_empty());
    assert!(emptied_out.plist().splits().is_empty());
    assert_eq!(emptied_out.new_min_ts(), 4);
    Ok(())
}

/// A list whose pack holds the IDs numbered 1 to `n`.
fn pack_of(n: u64) -> TestResult<PostingList> {
    Ok(
        PostingList::default().with_pack(crate::utils::helpers::pack_of(
            &ids(&(1..=n).collect::<Vec<_>>())?,
            BLOCK_SIZE,
        )?),
    )
}

#[test]
fn test_a_list_splits_from_its_threshold_on() -> TestResult {
    // 256 uids fill one block of 256; 257 need two.
    let one_block = pack_of(256)?;
    assert_eq!(one_block.pack().context("a pack")?.blocks().len(), 1);
    assert!(!should_split(&one_block, 0), "one block never splits");
    let two_blocks = pack_of(257)?;
    assert_eq!(two_blocks.pack().context("a pack")?.blocks().len(), 2);
    let size = two_blocks.encoded_len();
    assert!(should_split(&two_blocks, size - 1));
    assert!(should_split(&two_blocks, size), "at the threshold");
    assert!(
        !should_split(&two_blocks, size + 1),
        "one byte below the threshold"
    );
    assert!(!should_split(&PostingList::default(), 0), "no pack");
    Ok(())
}

#[test]
fn test_a_list_is_split_at_its_middle_blocks_base() -> TestResult {
    // 1..=1000 in blocks of 256 start at 1, 257, 513 and 769; the middle block of four is the third, at 513. A
    // value's posting, keyed by its fingerprint below every ID, falls in the low half.
    let value = Posting::default().with_uid(7);
    let plist = pack_of(1000)?.with_postings(
        std::iter::once(value)
            .chain(
                [256, 512, 513, 1000]
                    .iter()
                    .map(|&n| Posting::default().with_uid(rid(n))),
            )
            .collect(),
    );
    let (starts, [low, high]) = bin_split(rid(100), &plist)?;
    assert_eq!(
        starts,
        [rid(100), rid(513)],
        "the low half keeps the start it was given"
    );
    assert_eq!(
        (
            low.pack().context("a pack")?.blocks().len(),
            high.pack().context("a pack")?.blocks().len()
        ),
        (2, 2)
    );
    assert_eq!(
        decode(low.pack().context("a pack")?, 0),
        ids(&(1..=512).collect::<Vec<_>>())?
    );
    assert_eq!(
        decode(high.pack().context("a pack")?, 0),
        ids(&(513..=1000).collect::<Vec<_>>())?
    );
    let uids = |p: &PostingList| p.postings().iter().map(|p| p.uid()).collect::<Vec<_>>();
    let mut low_keys = vec![7];
    low_keys.extend(rids(&[256, 512]));
    assert_eq!((uids(&low), uids(&high)), (low_keys, rids(&[513, 1000])));
    // Of three blocks, the middle is the second, at 257.
    let (three_block_starts, _) = bin_split(0, &pack_of(600)?)?;
    assert_eq!(three_block_starts, [0, rid(257)]);
    Ok(())
}

#[test]
fn test_a_rollup_deletes_only_the_parts_it_no_longer_has() -> TestResult {
    let mut l = List::empty(key("entries", 1));
    (1..=1000).for_each(|u| add(&mut l, 1, &set_uid(u)));
    l.commit_mutation(2);
    let out = l.rollup(3, true, 1000)?.context("a rollup")?;
    let splits = out.splits();
    let entries = out.entries(l.key(), &[0, rid(2)], 9)?;
    // The main key and every part are written; of the old parts, the first is still one, and the one at the ID
    // numbered 2 is deleted.
    assert_eq!(entries.len(), 1 + splits.len() + 1);
    let deletions: Vec<_> = entries.iter().filter(|e| e.is_deletion()).collect();
    assert_eq!(deletions.len(), 1);
    assert_eq!(deletions[0].key(), split_key_of(l.key(), rid(2))?);
    assert!(entries.iter().all(|e| e.version() == 9));
    assert!(entries[0].discard() && entries[0].meta() == BIT_COMPLETE_POSTING);
    // A part's key is made from the main key, which must be a list's.
    assert!(splits.len() > 1);
    assert_eq!(
        out.entries(b"\x17x", &[], 9)
            .failure("the entries of a key of no list")?
            .0,
        PostingErrorEnum::NotAList {
            key: b"\x17x".to_vec()
        }
    );
    Ok(())
}
