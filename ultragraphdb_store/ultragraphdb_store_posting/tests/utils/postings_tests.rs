/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Tests of the posting helpers ported from Dgraph v25.5.0, posting/list.go. Changed for ultragraphdb: written in
 * Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_codec::encode;
use ultragraphdb_proto_pb::pb::{Posting, PostingList, UidPack};
use ultragraphdb_store_posting::{
    BLOCK_SIZE, DEL, OVR, SET, delete_all_posting, has_delete_all, is_plist_empty, length_delta,
};

#[test]
fn test_delete_all_and_lengths() {
    let all = delete_all_posting();
    assert!(has_delete_all(&all));
    assert_eq!(all.uid, u64::MAX);
    let mut tagged = all.clone();
    tagged.lang_tag = b"en".to_vec();
    assert!(!has_delete_all(&tagged));
    assert_eq!(
        (length_delta(SET), length_delta(DEL), length_delta(OVR)),
        (1, -1, 0)
    );
}

#[test]
fn test_an_empty_list_has_no_uids_and_no_splits() {
    assert!(is_plist_empty(&PostingList::default()));
    assert!(!is_plist_empty(&PostingList {
        splits: vec![1],
        ..PostingList::default()
    }));
    assert!(!is_plist_empty(&PostingList {
        pack: encode(&[1], BLOCK_SIZE),
        ..PostingList::default()
    }));
}

#[test]
fn test_a_delete_all_is_a_delete_of_star_all_whatever_its_uid() {
    // Dgraph's createDeleteAllPosting: Op DEL, Value "_STAR_ALL", Uid math.MaxUint64, nothing else.
    let all = delete_all_posting();
    assert_eq!(
        all,
        Posting {
            op: DEL,
            value: b"_STAR_ALL".to_vec(),
            uid: u64::MAX,
            ..Posting::default()
        }
    );
    assert!(has_delete_all(&Posting {
        uid: 7,
        ..all.clone()
    }));
    assert!(!has_delete_all(&Posting {
        op: SET,
        ..all.clone()
    }));
    assert!(!has_delete_all(&Posting {
        op: OVR,
        ..all.clone()
    }));
    assert!(!has_delete_all(&Posting {
        value: b"_STAR_AL".to_vec(),
        ..all.clone()
    }));
    assert!(!has_delete_all(&Posting::default()));
}

#[test]
fn test_an_operation_of_no_kind_adds_nothing_to_a_length() {
    assert_eq!(
        (length_delta(0), length_delta(4), length_delta(u32::MAX)),
        (0, 0, 0)
    );
}

#[test]
fn test_a_pack_without_blocks_is_empty() {
    assert!(is_plist_empty(&PostingList {
        pack: Some(UidPack::default()),
        postings: vec![Posting::default()],
        ..PostingList::default()
    }));
}
