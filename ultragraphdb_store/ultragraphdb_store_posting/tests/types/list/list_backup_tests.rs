/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::{BackupPostingList, decode};
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_posting::{
    BIT_COMPLETE_POSTING, BIT_EMPTY_POSTING, List, PostingError, PostingErrorEnum,
    from_backup_posting_list,
};

use crate::utils::helpers::{add, del_uid, key, set_uid, with_facet};

#[test]
fn test_a_backup_holds_the_uids_as_uvarint_deltas() {
    let mut l = List::empty(key("backup-bytes", 1));
    [1, 3, 300]
        .iter()
        .for_each(|&u| add(&mut l, 1, &set_uid(u)));
    add(&mut l, 1, &with_facet(set_uid(3), "f"));
    l.commit_mutation(6);
    let (bl, entry) = l.to_backup_posting_list().unwrap().unwrap();
    // Hand-derived: the deltas 1, 2 and 297; 297 = 0x129 is the uvarint 0xa9 0x02.
    assert_eq!(bl.uid_bytes(), vec![0x01, 0x02, 0xa9, 0x02]);
    assert_eq!(
        bl.postings().len(),
        1,
        "the posting with a facet stays whole"
    );
    assert_eq!(
        bl.commit_ts(),
        0,
        "a rollup's list carries no commit timestamp"
    );
    assert_eq!(
        (entry.key(), entry.version(), entry.meta()),
        (l.key(), 6, BIT_COMPLETE_POSTING)
    );
    let plist = from_backup_posting_list(&bl).unwrap();
    assert_eq!(decode(plist.pack().unwrap(), 0), vec![1, 3, 300]);
    assert_eq!(plist.postings(), bl.postings());
}

#[test]
fn test_an_emptied_list_backs_up_empty() {
    let mut l = List::empty(key("backup-empty", 1));
    add(&mut l, 1, &set_uid(4));
    l.commit_mutation(2);
    add(&mut l, 3, &del_uid(4));
    l.commit_mutation(4);
    let (bl, entry) = l.to_backup_posting_list().unwrap().unwrap();
    assert!(bl.uid_bytes().is_empty() && bl.postings().is_empty());
    assert_eq!((entry.version(), entry.meta()), (4, BIT_EMPTY_POSTING));
    assert_eq!(from_backup_posting_list(&bl).unwrap().pack(), None);
}

#[test]
fn test_a_backup_of_uids_or_of_uid_bytes_reads_the_same_list() {
    let uids = BackupPostingList::default()
        .with_uids(vec![2, 5, 9])
        // Ignored when the uids are given.
        .with_uid_bytes(vec![0x07])
        .with_commit_ts(11)
        .with_splits(vec![1]);
    // Hand-derived: the deltas 2, 3, 4.
    let bytes = BackupPostingList::default().with_uid_bytes(vec![0x02, 0x03, 0x04]);
    let from_uids = from_backup_posting_list(&uids).unwrap();
    let from_bytes = from_backup_posting_list(&bytes).unwrap();
    assert_eq!(decode(from_uids.pack().unwrap(), 0), vec![2, 5, 9]);
    assert_eq!(from_uids.pack(), from_bytes.pack());
    assert_eq!((from_uids.commit_ts(), from_uids.splits()), (11, &[1][..]));
}

#[test]
fn test_uid_bytes_that_end_inside_a_varint_are_refused() {
    // Deviation: Dgraph's loop never ends on a truncated uvarint; the port refuses it as a corrupted value.
    let truncated = BackupPostingList::default().with_uid_bytes(vec![0x01, 0x80]);
    match from_backup_posting_list(&truncated) {
        Err(PostingError(PostingErrorEnum::Store(StoreError(StoreErrorEnum::Corrupted {
            key,
            ..
        })))) => assert!(key.is_empty(), "a backup's list names no key"),
        other => panic!("not refused: {other:?}"),
    }
}
