/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Tests of the posting helpers ported from Dgraph v25.5.0, posting/list.go. Changed for ultragraphdb: written in
 * Rust; the identities follow list.go:835-855.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_common_utils::go_farm::fingerprint64;
use ultragraphdb_core_codec::encode;
use ultragraphdb_proto_pb::pb::{DirectedEdge, PostingList, directed_edge, posting};
use ultragraphdb_store_posting::{
    BLOCK_SIZE, DEL, OVR, PostingErrorEnum, SET, delete_all_posting, fingerprint_edge,
    has_delete_all, is_plist_empty, length_delta, new_posting,
};

#[test]
fn test_new_posting_maps_operations_and_types() {
    let reference = DirectedEdge {
        value_id: 7,
        op: directed_edge::Op::Set as i32,
        ..DirectedEdge::default()
    };
    let p = new_posting(&reference).unwrap();
    assert_eq!(
        (p.uid, p.op, p.posting_type),
        (7, SET, posting::PostingType::Ref as i32)
    );

    let value = DirectedEdge {
        value: b"v".to_vec(),
        op: directed_edge::Op::Del as i32,
        ..DirectedEdge::default()
    };
    let p = new_posting(&value).unwrap();
    assert_eq!(
        (p.op, p.posting_type),
        (DEL, posting::PostingType::Value as i32)
    );

    let lang = DirectedEdge {
        lang: "fr".into(),
        op: directed_edge::Op::Ovr as i32,
        ..value.clone()
    };
    let p = new_posting(&lang).unwrap();
    assert_eq!(
        (p.op, p.posting_type, p.lang_tag.as_slice()),
        (
            OVR,
            posting::PostingType::ValueLang as i32,
            b"fr".as_slice()
        )
    );
}

#[test]
fn test_an_unknown_operation_is_an_error_where_dgraph_exits() {
    // Deviation: Dgraph's NewPosting calls x.Fatalf("Unhandled operation") and exits the process.
    let edge = DirectedEdge {
        op: 9,
        ..DirectedEdge::default()
    };
    assert_eq!(
        new_posting(&edge).unwrap_err().0,
        PostingErrorEnum::UnhandledOperation { op: 9 }
    );
}

#[test]
fn test_posting_identities() {
    let untagged = DirectedEdge {
        value: b"A".to_vec(),
        ..DirectedEdge::default()
    };
    assert_eq!(fingerprint_edge(&untagged, false), u64::MAX);
    assert_eq!(fingerprint_edge(&untagged, true), fingerprint64(b"A"));
    let tagged = DirectedEdge {
        lang: "fr".into(),
        ..untagged
    };
    assert_eq!(fingerprint_edge(&tagged, true), fingerprint64(b"fr"));
}

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
