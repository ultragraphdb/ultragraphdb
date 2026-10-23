/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::encode;
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_proto_pb::pb::{Posting, PostingList, UidBlock, UidPack, posting};
use ultragraphdb_store_posting::{
    BLOCK_SIZE, DEL, PostingError, PostingErrorEnum, SET, marshal, unmarshal_complete,
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
