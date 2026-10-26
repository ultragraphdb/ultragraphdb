/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::encode;
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_proto_pb::pb::{Posting, PostingList, UidBlock, UidPack, posting};
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
    Posting {
        uid,
        value: b"v".to_vec(),
        posting_type: posting::PostingType::Value as i32,
        op: SET,
        ..Posting::default()
    }
}

#[test]
fn test_a_complete_list_round_trips() {
    let plist = PostingList {
        pack: encode(&[1, 5, u64::MAX], BLOCK_SIZE),
        postings: vec![value_posting(u64::MAX)],
        ..PostingList::default()
    };
    assert_eq!(unmarshal_complete(KEY, &marshal(&plist)).unwrap(), plist);
    assert_eq!(
        unmarshal_complete(KEY, &[]).unwrap(),
        PostingList::default()
    );
}

#[test]
fn test_a_delta_round_trips() {
    let delta = PostingList {
        postings: vec![
            Posting {
                uid: 9,
                op: DEL,
                ..Posting::default()
            },
            Posting {
                uid: 3,
                op: SET,
                ..Posting::default()
            },
        ],
        ..PostingList::default()
    };
    assert_eq!(unmarshal_delta(KEY, &marshal(&delta)).unwrap(), delta);
}

#[test]
fn test_bytes_that_do_not_decode_are_refused() {
    refused(unmarshal_complete(KEY, &[0xff, 0xff, 0xff]));
    refused(unmarshal_delta(KEY, &[0x0a, 0x05, 0x01]));
}

#[test]
fn test_a_delta_with_a_pack_is_refused() {
    let delta = PostingList {
        pack: encode(&[1], BLOCK_SIZE),
        ..PostingList::default()
    };
    assert!(refused(unmarshal_delta(KEY, &marshal(&delta))).contains("pack"));
}

#[test]
fn test_unknown_operations_and_types_are_refused() {
    let delta = PostingList {
        postings: vec![Posting {
            uid: 1,
            op: 9,
            ..Posting::default()
        }],
        ..PostingList::default()
    };
    refused(unmarshal_delta(KEY, &marshal(&delta)));
    let complete = PostingList {
        pack: encode(&[1], BLOCK_SIZE),
        postings: vec![Posting {
            uid: 1,
            val_type: 99,
            ..Posting::default()
        }],
        ..PostingList::default()
    };
    refused(unmarshal_complete(KEY, &marshal(&complete)));
}

#[test]
fn test_a_pack_whose_uids_do_not_ascend_is_refused() {
    let mut pack = encode(&[1, 2, 1 << 33], BLOCK_SIZE).unwrap();
    assert_eq!(pack.blocks.len(), 2);
    pack.blocks.reverse();
    let plist = PostingList {
        pack: Some(pack),
        ..PostingList::default()
    };
    assert!(refused(unmarshal_complete(KEY, &marshal(&plist))).contains("ascend"));
}

#[test]
fn test_a_block_claiming_more_uids_than_its_size_is_refused() {
    let pack = UidPack {
        block_size: 4,
        blocks: vec![UidBlock {
            base: 1,
            deltas: vec![0; 17],
            num_uids: u32::MAX,
        }],
        alloc_ref: 0,
    };
    let plist = PostingList {
        pack: Some(pack),
        ..PostingList::default()
    };
    assert!(refused(unmarshal_complete(KEY, &marshal(&plist))).contains("block"));
}

#[test]
fn test_postings_out_of_order_or_outside_the_pack_are_refused() {
    let unsorted = PostingList {
        pack: encode(&[1, 2], BLOCK_SIZE),
        postings: vec![value_posting(2), value_posting(1)],
        ..PostingList::default()
    };
    assert!(refused(unmarshal_complete(KEY, &marshal(&unsorted))).contains("sorted"));
    let outside = PostingList {
        pack: encode(&[1, 2], BLOCK_SIZE),
        postings: vec![value_posting(3)],
        ..PostingList::default()
    };
    assert!(refused(unmarshal_complete(KEY, &marshal(&outside))).contains("pack"));
}

#[test]
fn test_splits_must_ascend_from_one_and_stand_alone() {
    let split = PostingList {
        splits: vec![1, 100],
        ..PostingList::default()
    };
    assert_eq!(unmarshal_complete(KEY, &marshal(&split)).unwrap(), split);
    let from_two = PostingList {
        splits: vec![2, 100],
        ..PostingList::default()
    };
    refused(unmarshal_complete(KEY, &marshal(&from_two)));
    let with_uids = PostingList {
        splits: vec![1],
        pack: encode(&[1], BLOCK_SIZE),
        ..PostingList::default()
    };
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

#[test]
fn test_a_delta_has_its_protobuf_encoding() {
    // Hand-derived from pb.proto: PostingList.postings is field 2, length-delimited (0x12), 11 bytes; Posting.uid is
    // field 1, fixed64 (0x09), 3 little-endian; Posting.op is field 12, varint ((12 << 3) | 0 = 0x60), 1.
    let golden = [0x12, 0x0b, 0x09, 0x03, 0, 0, 0, 0, 0, 0, 0, 0x60, 0x01];
    let delta = PostingList {
        postings: vec![Posting {
            uid: 3,
            op: SET,
            ..Posting::default()
        }],
        ..PostingList::default()
    };
    assert_eq!(marshal(&delta), golden);
    assert_eq!(unmarshal_delta(KEY, &golden).unwrap(), delta);
}

#[test]
fn test_a_complete_list_has_its_protobuf_encoding() {
    // Hand-derived from pb.proto: PostingList.pack is field 1 (0x0a), 9 bytes; UidPack.block_size field 1 varint
    // (0x08) 256 = 0x80 0x02; UidPack.blocks field 2 (0x12), 4 bytes; UidBlock.base field 1 (0x08) 5, num_uids field
    // 3 (0x18) 1; PostingList.commit_ts field 3 (0x18) 7.
    let golden = [
        0x0a, 0x09, 0x08, 0x80, 0x02, 0x12, 0x04, 0x08, 0x05, 0x18, 0x01, 0x18, 0x07,
    ];
    let complete = PostingList {
        pack: Some(UidPack {
            block_size: 256,
            blocks: vec![UidBlock {
                base: 5,
                deltas: Vec::new(),
                num_uids: 1,
            }],
            alloc_ref: 0,
        }),
        commit_ts: 7,
        ..PostingList::default()
    };
    assert_eq!(marshal(&complete), golden);
    assert_eq!(unmarshal_complete(KEY, &golden).unwrap(), complete);
}

#[test]
fn test_a_delta_with_splits_is_refused() {
    let delta = PostingList {
        splits: vec![1],
        ..PostingList::default()
    };
    assert_eq!(
        refused(unmarshal_delta(KEY, &marshal(&delta))),
        "a delta carries a pack or splits"
    );
}

#[test]
fn test_a_delta_needs_an_operation_and_a_complete_list_holds_no_delete() {
    let with_op = |op: u32| PostingList {
        postings: vec![Posting {
            uid: 1,
            op,
            ..Posting::default()
        }],
        ..PostingList::default()
    };
    for op in [SET, DEL, OVR] {
        assert!(
            unmarshal_delta(KEY, &marshal(&with_op(op))).is_ok(),
            "op {op}"
        );
    }
    assert_eq!(
        refused(unmarshal_delta(KEY, &marshal(&with_op(0)))),
        "posting 1 has an unknown type or operation"
    );
    let complete = |op: u32| PostingList {
        pack: encode(&[1], BLOCK_SIZE),
        ..with_op(op)
    };
    for op in [0, SET, OVR] {
        assert_eq!(
            unmarshal_complete(KEY, &marshal(&complete(op))).unwrap(),
            complete(op),
            "op {op}"
        );
    }
    assert_eq!(
        refused(unmarshal_complete(KEY, &marshal(&complete(DEL)))),
        "posting 1 has an unknown type or operation"
    );
    let unknown_type = PostingList {
        postings: vec![Posting {
            uid: 1,
            op: SET,
            posting_type: 3,
            ..Posting::default()
        }],
        ..PostingList::default()
    };
    refused(unmarshal_delta(KEY, &marshal(&unknown_type)));
}

#[test]
fn test_splits_must_strictly_ascend_and_a_split_list_holds_no_postings() {
    let one = PostingList {
        splits: vec![1],
        ..PostingList::default()
    };
    assert_eq!(unmarshal_complete(KEY, &marshal(&one)).unwrap(), one);
    let repeated = PostingList {
        splits: vec![1, 1],
        ..PostingList::default()
    };
    assert_eq!(
        refused(unmarshal_complete(KEY, &marshal(&repeated))),
        "the splits of a list do not ascend from 1"
    );
    let descending = PostingList {
        splits: vec![1, 9, 5],
        ..PostingList::default()
    };
    refused(unmarshal_complete(KEY, &marshal(&descending)));
    let with_postings = PostingList {
        splits: vec![1],
        postings: vec![value_posting(1)],
        ..PostingList::default()
    };
    assert_eq!(
        refused(unmarshal_complete(KEY, &marshal(&with_postings))),
        "a split list carries uids of its own"
    );
}

#[test]
fn test_a_block_holds_between_one_uid_and_the_block_size() {
    let pack = |block_size: u32, num_uids: u32, deltas: Vec<u8>| PostingList {
        pack: Some(UidPack {
            block_size,
            blocks: vec![UidBlock {
                base: 1,
                deltas,
                num_uids,
            }],
            alloc_ref: 0,
        }),
        ..PostingList::default()
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
    assert_eq!((full.blocks.len(), full.blocks[0].num_uids), (1, 4));
    let full = PostingList {
        pack: Some(full),
        ..PostingList::default()
    };
    assert_eq!(unmarshal_complete(KEY, &marshal(&full)).unwrap(), full);
    // A block size of zero bounds a block at one uid.
    assert_eq!(
        unmarshal_complete(KEY, &marshal(&pack(0, 1, Vec::new()))).unwrap(),
        pack(0, 1, Vec::new())
    );
}

#[test]
fn test_a_uid_repeated_across_blocks_is_refused() {
    let mut pack = encode(&[1, 2, 1 << 33], BLOCK_SIZE).unwrap();
    assert_eq!(pack.blocks.len(), 2);
    pack.blocks[1] = pack.blocks[0].clone();
    let plist = PostingList {
        pack: Some(pack),
        ..PostingList::default()
    };
    assert_eq!(
        refused(unmarshal_complete(KEY, &marshal(&plist))),
        "the uids of a pack do not ascend"
    );
}

#[test]
fn test_literals_sharing_a_fingerprint_sort_by_encoding_in_a_quad_list_alone() {
    let literal = |value: &[u8]| Posting {
        value: value.to_vec(),
        ..value_posting(9)
    };
    let shared = |postings: Vec<Posting>| PostingList {
        pack: encode(&[9, 12], BLOCK_SIZE),
        postings,
        ..PostingList::default()
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
