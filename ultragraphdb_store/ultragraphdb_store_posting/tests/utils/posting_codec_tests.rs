/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::{Posting, PostingList, PostingType, UidBlock, UidPack};
use ultragraphdb_core_keys::{Graph, QuadKey};
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_posting::{
    BLOCK_SIZE, DEL, OVR, PostingError, PostingErrorEnum, SET, marshal, unmarshal_complete,
    unmarshal_delta,
};
use ultragraphdb_tool_bdd::{Context, TestResult};
use xxhash_rust::xxh3::xxh3_64;

use crate::utils::helpers::{id, ids, pack_of, rid};

const KEY: &[u8] = b"\x00key";

fn refused(result: Result<PostingList, PostingError>) -> TestResult<String> {
    let (key, reason) = match result {
        Err(PostingError(PostingErrorEnum::Store(StoreError(StoreErrorEnum::Corrupted {
            key,
            reason,
        })))) => Ok((key, reason)),
        other => Err(format!("not refused as corrupted: {other:?}")),
    }
    .context("the list is refused as corrupted")?;
    assert_eq!(key, KEY);
    Ok(reason)
}

/// A value posting keyed by the fingerprint `fingerprint`.
fn value_posting(fingerprint: u64) -> Posting {
    Posting::default()
        .with_uid(u128::from(fingerprint))
        .with_value(b"v".to_vec())
        .with_posting_type(PostingType::Value)
        .with_op(SET)
}

#[test]
fn test_a_complete_list_round_trips() -> TestResult {
    let plist = PostingList::default()
        .with_pack(pack_of(&ids(&[1, 5, u64::MAX])?, BLOCK_SIZE)?)
        .with_postings(vec![
            value_posting(u64::MAX),
            Posting::default().with_uid(rid(5)).with_op(SET),
        ]);
    assert_eq!(unmarshal_complete(KEY, &marshal(&plist))?, plist);
    // The posting format writes even an empty list's version, counts and checksum, so empty bytes are no list: a
    // protobuf encoding of the empty list was empty.
    assert_eq!(
        refused(unmarshal_complete(KEY, &[]))?,
        "the value is 0 bytes, shorter than the 26 of an empty posting list"
    );
    Ok(())
}

#[test]
fn test_a_delta_round_trips() -> TestResult {
    let delta = PostingList::default().with_postings(vec![
        Posting::default().with_uid(rid(9)).with_op(DEL),
        Posting::default().with_uid(rid(3)).with_op(SET),
    ]);
    assert_eq!(unmarshal_delta(KEY, &marshal(&delta))?, delta);
    Ok(())
}

#[test]
fn test_bytes_that_do_not_decode_are_refused() -> TestResult {
    let _complete_reason: String = refused(unmarshal_complete(KEY, &[0xff, 0xff, 0xff]))?;
    let _delta_reason: String = refused(unmarshal_delta(KEY, &[0x0a, 0x05, 0x01]))?;
    Ok(())
}

#[test]
fn test_a_flipped_byte_of_a_stored_value_is_refused_naming_its_key() -> TestResult {
    let delta = PostingList::default().with_postings(vec![value_posting(4)]);
    let bytes = marshal(&delta);
    for at in 0..bytes.len() {
        let mut flipped = bytes.clone();
        flipped[at] ^= 0x10;
        let _delta_reason: String = refused(unmarshal_delta(KEY, &flipped))?;
        let _complete_reason: String = refused(unmarshal_complete(KEY, &flipped))?;
    }
    Ok(())
}

#[test]
fn test_a_delta_with_a_pack_is_refused() -> TestResult {
    let delta = PostingList::default().with_pack(pack_of(&[id(1)?], BLOCK_SIZE)?);
    assert!(refused(unmarshal_delta(KEY, &marshal(&delta)))?.contains("pack"));
    Ok(())
}

#[test]
fn test_unknown_operations_are_refused() -> TestResult {
    let delta =
        PostingList::default().with_postings(vec![Posting::default().with_uid(rid(1)).with_op(9)]);
    let _delta_reason: String = refused(unmarshal_delta(KEY, &marshal(&delta)))?;
    let complete = PostingList::default()
        .with_pack(pack_of(&[id(1)?], BLOCK_SIZE)?)
        .with_postings(vec![Posting::default().with_uid(rid(1)).with_op(9)]);
    let _complete_reason: String = refused(unmarshal_complete(KEY, &marshal(&complete)))?;
    Ok(())
}

/// The stored bytes of a list whose pack holds the two blocks of the IDs 1 to 4 (block size 2) in the order `order`
/// names, written from the posting format's layout: the list's own encoding with its blocks' bytes rearranged and
/// the checksum recomputed. No `UidPack` holds blocks out of order, so only bytes can.
fn two_blocks_in_order(order: [usize; 2]) -> TestResult<Vec<u8>> {
    let pack = pack_of(&ids(&[1, 2, 3, 4])?, 2)?;
    assert_eq!(pack.blocks().len(), 2);
    // Each block: its first ID (16 bytes), its count and its deltas' length (4 each) and its one delta, LEB128 of the
    // 104-bit difference 2^24, 4 bytes.
    let block_len = 16 + 4 + 4 + 4;
    assert!(pack.blocks().iter().all(|b| b.deltas().len() == 4));
    let bytes = marshal(&PostingList::default().with_pack(pack));
    // Version and flags, the commit version, then the pack's version, block size and block count: 19 bytes.
    let head = 19;
    let block = |i: usize| bytes.get(head + i * block_len..head + (i + 1) * block_len);
    let mut body = bytes.get(..head).context("the head")?.to_vec();
    for i in order {
        body.extend_from_slice(block(i).context("a block")?);
    }
    let end = bytes.len() - 8;
    body.extend_from_slice(bytes.get(head + 2 * block_len..end).context("the tail")?);
    let checksum = xxh3_64(&body).to_le_bytes();
    body.extend_from_slice(&checksum);
    Ok(body)
}

#[test]
fn test_a_pack_whose_uids_do_not_ascend_is_refused() -> TestResult {
    assert!(unmarshal_complete(KEY, &two_blocks_in_order([0, 1])?).is_ok());
    assert_eq!(
        refused(unmarshal_complete(KEY, &two_blocks_in_order([1, 0])?))?,
        "the pack is corrupt: block 1 does not start above the block before it"
    );
    Ok(())
}

#[test]
fn test_a_block_claiming_more_uids_than_its_size_is_refused() -> TestResult {
    let five = pack_of(&ids(&[1, 2, 3, 4, 5])?, 5)?;
    let pack = UidPack::new(4, five.blocks().to_vec()).context("the pack")?;
    let plist = PostingList::default().with_pack(pack);
    assert!(refused(unmarshal_complete(KEY, &marshal(&plist)))?.contains("block"));
    Ok(())
}

#[test]
fn test_postings_out_of_order_or_outside_the_pack_are_refused() -> TestResult {
    let pack = || pack_of(&ids(&[1, 2])?, BLOCK_SIZE);
    let unsorted = PostingList::default()
        .with_pack(pack()?)
        .with_postings(vec![value_posting(2), value_posting(1)]);
    assert!(refused(unmarshal_complete(KEY, &marshal(&unsorted)))?.contains("sorted"));
    let outside = PostingList::default()
        .with_pack(pack()?)
        .with_postings(vec![Posting::default().with_uid(rid(3)).with_op(SET)]);
    assert_eq!(
        refused(unmarshal_complete(KEY, &marshal(&outside)))?,
        format!("posting {} is not in the list's pack", rid(3))
    );
    // A value's key is its fingerprint, which no pack holds.
    let value = PostingList::default()
        .with_pack(pack()?)
        .with_postings(vec![value_posting(3)]);
    assert_eq!(unmarshal_complete(KEY, &marshal(&value))?, value);
    Ok(())
}

#[test]
fn test_splits_must_ascend_from_the_first_start_and_stand_alone() -> TestResult {
    // A list but a value index's starts at 0, the smallest fingerprint; its later parts at fingerprints or IDs.
    let split = PostingList::default().with_splits(vec![0, 100, rid(7)]);
    assert_eq!(unmarshal_complete(KEY, &marshal(&split))?, split);
    let from_two = PostingList::default().with_splits(vec![2, 100]);
    let _from_two_reason: String = refused(unmarshal_complete(KEY, &marshal(&from_two)))?;
    let with_uids = PostingList::default()
        .with_splits(vec![0])
        .with_pack(pack_of(&[id(1)?], BLOCK_SIZE)?);
    let _with_uids_reason: String = refused(unmarshal_complete(KEY, &marshal(&with_uids)))?;
    Ok(())
}

/// A forward key of the quad layout, whose literals sharing a fingerprint sort by their encodings.
fn quad_key() -> TestResult<Vec<u8>> {
    Ok(QuadKey::Forward {
        namespace: 0,
        graph: Graph::Default,
        predicate: id(7)?,
        subject: id(1)?,
    }
    .to_bytes())
}

fn unhex(text: &str) -> TestResult<Vec<u8>> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).context("hex"))
        .collect()
}

#[test]
fn test_a_delta_has_its_posting_format_encoding() -> TestResult {
    // Written from the posting format's layout: version 2, no flags, commit version 0, one posting (the key of the ID
    // of type 1, sequence 3 and bucket 0 as 16 big-endian bytes, op 1, a reference, both timestamps 0, every length
    // and count 0), no splits; the checksum is python-xxhash 3.6.0's XXH3-64 (libxxhash 0.8.2).
    let golden = unhex(
        "0200000000000000000001000000000100000000000000030000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000d54f54ff80ca282a",
    )?;
    let delta = PostingList::default()
        .with_postings(vec![Posting::default().with_uid(rid(3)).with_op(SET)]);
    assert_eq!(marshal(&delta), golden);
    assert_eq!(unmarshal_delta(KEY, &golden)?, delta);
    Ok(())
}

#[test]
fn test_a_complete_list_has_its_posting_format_encoding() -> TestResult {
    // Written from the posting format's layout: version 2, the pack flag, commit version 7, the pack (version 1, block
    // size 256, one block: the ID of type 1, sequence 5 and bucket 0, one ID, no deltas), no postings, no splits; the
    // checksum is python-xxhash 3.6.0's XXH3-64 (libxxhash 0.8.2).
    let golden = unhex(
        "020107000000000000000100010000010000000001000000000000000500000000000001000000000000000000000000000000b2efa002be9f5934",
    )?;
    let block = UidBlock::new(id(5)?, Vec::new(), 1).context("the block")?;
    let complete = PostingList::default()
        .with_pack(UidPack::new(256, vec![block]).context("the pack")?)
        .with_commit_ts(7);
    assert_eq!(marshal(&complete), golden);
    assert_eq!(unmarshal_complete(KEY, &golden)?, complete);
    Ok(())
}

#[test]
fn test_a_delta_with_splits_is_refused() -> TestResult {
    let delta = PostingList::default().with_splits(vec![0]);
    assert_eq!(
        refused(unmarshal_delta(KEY, &marshal(&delta)))?,
        "a delta carries a pack or splits"
    );
    Ok(())
}

#[test]
fn test_a_delta_needs_an_operation_and_a_complete_list_holds_no_delete() -> TestResult {
    let with_op = |op: u32| {
        PostingList::default().with_postings(vec![Posting::default().with_uid(rid(1)).with_op(op)])
    };
    for op in [SET, DEL, OVR] {
        assert!(
            unmarshal_delta(KEY, &marshal(&with_op(op))).is_ok(),
            "op {op}"
        );
    }
    assert_eq!(
        refused(unmarshal_delta(KEY, &marshal(&with_op(0))))?,
        format!("posting {} has an unknown operation", rid(1))
    );
    let complete = |op: u32| -> TestResult<PostingList> {
        Ok(with_op(op).with_pack(pack_of(&[id(1)?], BLOCK_SIZE)?))
    };
    for op in [0, SET, OVR] {
        assert_eq!(
            unmarshal_complete(KEY, &marshal(&complete(op)?))?,
            complete(op)?,
            "op {op}"
        );
    }
    assert_eq!(
        refused(unmarshal_complete(KEY, &marshal(&complete(DEL)?)))?,
        format!("posting {} has an unknown operation", rid(1))
    );
    Ok(())
}

#[test]
fn test_splits_must_strictly_ascend_and_a_split_list_holds_no_postings() -> TestResult {
    let one = PostingList::default().with_splits(vec![0]);
    assert_eq!(unmarshal_complete(KEY, &marshal(&one))?, one);
    let repeated = PostingList::default().with_splits(vec![0, 0]);
    assert_eq!(
        refused(unmarshal_complete(KEY, &marshal(&repeated)))?,
        "the splits of a list do not ascend from its first start"
    );
    let descending = PostingList::default().with_splits(vec![0, 9, 5]);
    let _reason: String = refused(unmarshal_complete(KEY, &marshal(&descending)))?;
    let with_postings = PostingList::default()
        .with_splits(vec![0])
        .with_postings(vec![value_posting(1)]);
    assert_eq!(
        refused(unmarshal_complete(KEY, &marshal(&with_postings)))?,
        "a split list carries uids of its own"
    );
    Ok(())
}

#[test]
fn test_a_block_holds_between_one_uid_and_the_block_size() -> TestResult {
    let pack = |block_size: u32, block: UidBlock| -> TestResult<PostingList> {
        Ok(PostingList::default()
            .with_pack(UidPack::new(block_size, vec![block]).context("the pack")?))
    };
    // An empty block is the codec's, which no encoder of the store writes.
    let empty = UidBlock::new(id(1)?, Vec::new(), 0).context("the empty block")?;
    assert_eq!(
        refused(unmarshal_complete(KEY, &marshal(&pack(4, empty)?)))?,
        "a block holds no uid or more than the block size"
    );
    let five = pack_of(&ids(&[1, 2, 3, 4, 5])?, 5)?;
    let five = five.blocks().first().context("the block of five")?.clone();
    assert_eq!(
        refused(unmarshal_complete(KEY, &marshal(&pack(4, five)?)))?,
        "a block holds no uid or more than the block size"
    );
    // At the block size: the codec's own block of four IDs 1, 2, 3, 4.
    let full = pack_of(&ids(&[1, 2, 3, 4])?, 4)?;
    assert_eq!(
        (
            full.blocks().len(),
            full.blocks().first().map(UidBlock::count)
        ),
        (1, Some(4))
    );
    let full = PostingList::default().with_pack(full);
    assert_eq!(unmarshal_complete(KEY, &marshal(&full))?, full);
    // A block size of zero bounds a block at one ID.
    let one = || UidBlock::new(id(1)?, Vec::new(), 1).context("the block of one");
    assert_eq!(
        unmarshal_complete(KEY, &marshal(&pack(0, one()?)?))?,
        pack(0, one()?)?
    );
    Ok(())
}

#[test]
fn test_a_uid_repeated_across_blocks_is_refused() -> TestResult {
    assert_eq!(
        refused(unmarshal_complete(KEY, &two_blocks_in_order([0, 0])?))?,
        "the pack is corrupt: block 1 does not start above the block before it"
    );
    Ok(())
}

#[test]
fn test_literals_sharing_a_fingerprint_sort_by_encoding_in_a_quad_list_alone() -> TestResult {
    let literal = |value: &[u8]| value_posting(9).with_value(value.to_vec());
    let reference = Posting::default().with_uid(rid(12)).with_op(SET);
    let shared = |postings: Vec<Posting>| -> TestResult<PostingList> {
        Ok(PostingList::default()
            .with_pack(pack_of(&[id(12)?], BLOCK_SIZE)?)
            .with_postings(postings))
    };
    let sorted = shared(vec![literal(b"a"), literal(b"b"), reference.clone()])?;
    let key = quad_key()?;
    assert_eq!(unmarshal_complete(&key, &marshal(&sorted))?, sorted);
    for unsorted in [
        shared(vec![literal(b"b"), literal(b"a")])?,
        shared(vec![literal(b"a"), literal(b"a")])?,
    ] {
        let reason = match unmarshal_complete(&key, &marshal(&unsorted)) {
            Err(PostingError(PostingErrorEnum::Store(StoreError(StoreErrorEnum::Corrupted {
                reason,
                ..
            })))) => Ok(reason),
            other => Err(format!("not refused: {other:?}")),
        }
        .context("the unsorted list is refused as corrupted")?;
        assert_eq!(reason, "the postings of a list are not sorted by uid");
    }
    // A key that does not compare values holds one posting per key.
    assert_eq!(
        refused(unmarshal_complete(KEY, &marshal(&sorted)))?,
        "the postings of a list are not sorted by uid"
    );
    // The reference after a shared fingerprint must still be in the pack.
    let outside = shared(vec![
        literal(b"a"),
        literal(b"b"),
        Posting::default().with_uid(rid(13)).with_op(SET),
    ])?;
    assert!(unmarshal_complete(&key, &marshal(&outside)).is_err());
    Ok(())
}
