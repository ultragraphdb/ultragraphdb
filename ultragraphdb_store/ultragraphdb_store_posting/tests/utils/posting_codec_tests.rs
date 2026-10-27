/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::{Posting, PostingList, PostingType, UidBlock, UidPack, encode};
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_posting::{
    BLOCK_SIZE, DEL, OVR, PostingError, PostingErrorEnum, SET, marshal, unmarshal_complete,
    unmarshal_delta,
};

const KEY: &[u8] = b"\x00key";

fn refused(result: Result<PostingList, PostingError>) -> String {
    match result {
        Err(PostingError(PostingErrorEnum::Store(StoreError(StoreErrorEnum::Corrupted {
            key,
            reason,
        })))) => {
            assert_eq!(key, KEY);
            reason
        }
        other => panic!("not refused as corrupted: {other:?}"),
    }
}

fn value_posting(uid: u64) -> Posting {
    Posting::default()
        .with_uid(uid)
        .with_value(b"v".to_vec())
        .with_posting_type(PostingType::Value)
        .with_op(SET)
}

#[test]
fn test_a_complete_list_round_trips() {
    let plist = PostingList::default()
        .with_pack(encode(&[1, 5, u64::MAX], BLOCK_SIZE).expect("the uids pack"))
        .with_postings(vec![value_posting(u64::MAX)]);
    assert_eq!(unmarshal_complete(KEY, &marshal(&plist)).unwrap(), plist);
    // The posting format writes even an empty list's version, counts and checksum, so empty bytes are no list: a
    // protobuf encoding of the empty list was empty.
    assert_eq!(
        refused(unmarshal_complete(KEY, &[])),
        "the value is 0 bytes, shorter than the 26 of an empty posting list"
    );
}

#[test]
fn test_a_delta_round_trips() {
    let delta = PostingList::default().with_postings(vec![
        Posting::default().with_uid(9).with_op(DEL),
        Posting::default().with_uid(3).with_op(SET),
    ]);
    assert_eq!(unmarshal_delta(KEY, &marshal(&delta)).unwrap(), delta);
}

#[test]
fn test_bytes_that_do_not_decode_are_refused() {
    refused(unmarshal_complete(KEY, &[0xff, 0xff, 0xff]));
    refused(unmarshal_delta(KEY, &[0x0a, 0x05, 0x01]));
}

#[test]
fn test_a_flipped_byte_of_a_stored_value_is_refused_naming_its_key() {
    let delta = PostingList::default().with_postings(vec![value_posting(4)]);
    let bytes = marshal(&delta);
    for at in 0..bytes.len() {
        let mut flipped = bytes.clone();
        flipped[at] ^= 0x10;
        refused(unmarshal_delta(KEY, &flipped));
        refused(unmarshal_complete(KEY, &flipped));
    }
}

#[test]
fn test_a_delta_with_a_pack_is_refused() {
    let delta = PostingList::default().with_pack(encode(&[1], BLOCK_SIZE).expect("the uids pack"));
    assert!(refused(unmarshal_delta(KEY, &marshal(&delta))).contains("pack"));
}

#[test]
fn test_unknown_operations_are_refused() {
    let delta =
        PostingList::default().with_postings(vec![Posting::default().with_uid(1).with_op(9)]);
    refused(unmarshal_delta(KEY, &marshal(&delta)));
    let complete = PostingList::default()
        .with_pack(encode(&[1], BLOCK_SIZE).expect("the uids pack"))
        .with_postings(vec![Posting::default().with_uid(1).with_op(9)]);
    refused(unmarshal_complete(KEY, &marshal(&complete)));
}

#[test]
fn test_a_pack_whose_uids_do_not_ascend_is_refused() {
    let pack = encode(&[1, 2, 1 << 33], BLOCK_SIZE).unwrap();
    assert_eq!(pack.blocks().len(), 2);
    let mut blocks = pack.blocks().to_vec();
    blocks.reverse();
    let plist = PostingList::default().with_pack(UidPack::new(pack.block_size(), blocks));
    assert!(refused(unmarshal_complete(KEY, &marshal(&plist))).contains("ascend"));
}

#[test]
fn test_a_block_claiming_more_uids_than_its_size_is_refused() {
    let pack = UidPack::new(4, vec![UidBlock::new(1, vec![0; 17], u32::MAX)]);
    let plist = PostingList::default().with_pack(pack);
    assert!(refused(unmarshal_complete(KEY, &marshal(&plist))).contains("block"));
}

#[test]
fn test_postings_out_of_order_or_outside_the_pack_are_refused() {
    let unsorted = PostingList::default()
        .with_pack(encode(&[1, 2], BLOCK_SIZE).expect("the uids pack"))
        .with_postings(vec![value_posting(2), value_posting(1)]);
    assert!(refused(unmarshal_complete(KEY, &marshal(&unsorted))).contains("sorted"));
    let outside = PostingList::default()
        .with_pack(encode(&[1, 2], BLOCK_SIZE).expect("the uids pack"))
        .with_postings(vec![value_posting(3)]);
    assert!(refused(unmarshal_complete(KEY, &marshal(&outside))).contains("pack"));
}

#[test]
fn test_splits_must_ascend_from_one_and_stand_alone() {
    let split = PostingList::default().with_splits(vec![1, 100]);
    assert_eq!(unmarshal_complete(KEY, &marshal(&split)).unwrap(), split);
    let from_two = PostingList::default().with_splits(vec![2, 100]);
    refused(unmarshal_complete(KEY, &marshal(&from_two)));
    let with_uids = PostingList::default()
        .with_splits(vec![1])
        .with_pack(encode(&[1], BLOCK_SIZE).expect("the uids pack"));
    refused(unmarshal_complete(KEY, &marshal(&with_uids)));
}

/// A forward key of the quad layout, whose literals sharing a fingerprint sort by their encodings.
fn quad_key() -> Vec<u8> {
    ultragraphdb_core_keys::QuadKey::Forward {
        namespace: 0,
        graph: 0,
        predicate: 7,
        subject: 1,
    }
    .to_bytes()
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex"))
        .collect()
}

#[test]
fn test_a_delta_has_its_posting_format_encoding() {
    // Hand-derived from the posting format: version 1, no flags, commit timestamp 0, one posting (uid 3, op 1, a
    // reference, both timestamps 0, every length and count 0), no splits; the checksum is python-xxhash's XXH3-64.
    let golden = unhex(
        "010000000000000000000100000003000000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000302c128c5356a66f",
    );
    let delta =
        PostingList::default().with_postings(vec![Posting::default().with_uid(3).with_op(SET)]);
    assert_eq!(marshal(&delta), golden);
    assert_eq!(unmarshal_delta(KEY, &golden).unwrap(), delta);
}

#[test]
fn test_a_complete_list_has_its_posting_format_encoding() {
    // Hand-derived from the posting format: version 1, the pack flag, commit timestamp 7, block size 256, one block
    // (base 5, one uid, no deltas), no postings, no splits; the checksum is python-xxhash's XXH3-64.
    let golden = unhex(
        "010107000000000000000001000001000000050000000000000001000000000000000000000000000000338ea293b437c869",
    );
    let complete = PostingList::default()
        .with_pack(UidPack::new(256, vec![UidBlock::new(5, Vec::new(), 1)]))
        .with_commit_ts(7);
    assert_eq!(marshal(&complete), golden);
    assert_eq!(unmarshal_complete(KEY, &golden).unwrap(), complete);
}

#[test]
fn test_a_delta_with_splits_is_refused() {
    let delta = PostingList::default().with_splits(vec![1]);
    assert_eq!(
        refused(unmarshal_delta(KEY, &marshal(&delta))),
        "a delta carries a pack or splits"
    );
}

#[test]
fn test_a_delta_needs_an_operation_and_a_complete_list_holds_no_delete() {
    let with_op = |op: u32| {
        PostingList::default().with_postings(vec![Posting::default().with_uid(1).with_op(op)])
    };
    for op in [SET, DEL, OVR] {
        assert!(
            unmarshal_delta(KEY, &marshal(&with_op(op))).is_ok(),
            "op {op}"
        );
    }
    assert_eq!(
        refused(unmarshal_delta(KEY, &marshal(&with_op(0)))),
        "posting 1 has an unknown operation"
    );
    let complete =
        |op: u32| with_op(op).with_pack(encode(&[1], BLOCK_SIZE).expect("the uids pack"));
    for op in [0, SET, OVR] {
        assert_eq!(
            unmarshal_complete(KEY, &marshal(&complete(op))).unwrap(),
            complete(op),
            "op {op}"
        );
    }
    assert_eq!(
        refused(unmarshal_complete(KEY, &marshal(&complete(DEL)))),
        "posting 1 has an unknown operation"
    );
}

#[test]
fn test_splits_must_strictly_ascend_and_a_split_list_holds_no_postings() {
    let one = PostingList::default().with_splits(vec![1]);
    assert_eq!(unmarshal_complete(KEY, &marshal(&one)).unwrap(), one);
    let repeated = PostingList::default().with_splits(vec![1, 1]);
    assert_eq!(
        refused(unmarshal_complete(KEY, &marshal(&repeated))),
        "the splits of a list do not ascend from 1"
    );
    let descending = PostingList::default().with_splits(vec![1, 9, 5]);
    refused(unmarshal_complete(KEY, &marshal(&descending)));
    let with_postings = PostingList::default()
        .with_splits(vec![1])
        .with_postings(vec![value_posting(1)]);
    assert_eq!(
        refused(unmarshal_complete(KEY, &marshal(&with_postings))),
        "a split list carries uids of its own"
    );
}

#[test]
fn test_a_block_holds_between_one_uid_and_the_block_size() {
    let pack = |block_size: u32, num_uids: u32, deltas: Vec<u8>| {
        PostingList::default().with_pack(UidPack::new(
            block_size,
            vec![UidBlock::new(1, deltas, num_uids)],
        ))
    };
    assert_eq!(
        refused(unmarshal_complete(KEY, &marshal(&pack(4, 0, Vec::new())))),
        "a block holds no uid or more than the block size"
    );
    assert_eq!(
        refused(unmarshal_complete(KEY, &marshal(&pack(4, 5, Vec::new())))),
        "a block holds no uid or more than the block size"
    );
    // At the block size: the codec's own block of four uids 1, 2, 3, 4.
    let full = encode(&[1, 2, 3, 4], 4).unwrap();
    assert_eq!((full.blocks().len(), full.blocks()[0].num_uids()), (1, 4));
    let full = PostingList::default().with_pack(full);
    assert_eq!(unmarshal_complete(KEY, &marshal(&full)).unwrap(), full);
    // A block size of zero bounds a block at one uid.
    assert_eq!(
        unmarshal_complete(KEY, &marshal(&pack(0, 1, Vec::new()))).unwrap(),
        pack(0, 1, Vec::new())
    );
}

#[test]
fn test_a_uid_repeated_across_blocks_is_refused() {
    let pack = encode(&[1, 2, 1 << 33], BLOCK_SIZE).unwrap();
    assert_eq!(pack.blocks().len(), 2);
    let first = pack.blocks()[0].clone();
    let plist = PostingList::default()
        .with_pack(UidPack::new(pack.block_size(), vec![first.clone(), first]));
    assert_eq!(
        refused(unmarshal_complete(KEY, &marshal(&plist))),
        "the uids of a pack do not ascend"
    );
}

#[test]
fn test_literals_sharing_a_fingerprint_sort_by_encoding_in_a_quad_list_alone() {
    let literal = |value: &[u8]| value_posting(9).with_value(value.to_vec());
    let shared = |postings: Vec<Posting>| {
        PostingList::default()
            .with_pack(encode(&[9, 12], BLOCK_SIZE).expect("the uids pack"))
            .with_postings(postings)
    };
    let sorted = shared(vec![literal(b"a"), literal(b"b"), value_posting(12)]);
    let key = quad_key();
    assert_eq!(unmarshal_complete(&key, &marshal(&sorted)).unwrap(), sorted);
    for unsorted in [
        shared(vec![literal(b"b"), literal(b"a")]),
        shared(vec![literal(b"a"), literal(b"a")]),
    ] {
        match unmarshal_complete(&key, &marshal(&unsorted)) {
            Err(PostingError(PostingErrorEnum::Store(StoreError(StoreErrorEnum::Corrupted {
                reason,
                ..
            })))) => assert_eq!(reason, "the postings of a list are not sorted by uid"),
            other => panic!("not refused: {other:?}"),
        }
    }
    // A key that does not compare values holds one posting per uid.
    assert_eq!(
        refused(unmarshal_complete(KEY, &marshal(&sorted))),
        "the postings of a list are not sorted by uid"
    );
    // The posting after a shared uid must still be in the pack.
    let outside = shared(vec![literal(b"a"), literal(b"b"), value_posting(13)]);
    assert!(unmarshal_complete(&key, &marshal(&outside)).is_err());
}
