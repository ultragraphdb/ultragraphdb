/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Tests of the posting helpers ported from Dgraph v25.5.0, posting/list.go. Changed for ultragraphdb: written in
 * Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_codec::encode;
use ultragraphdb_proto_pb::pb::PostingList;
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
