/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::{BackupPostingList, Posting};

#[test]
fn a_backup_list_returns_what_it_was_built_with() {
    let postings = vec![Posting::default().with_uid(4)];
    let bl = BackupPostingList::default()
        .with_uids(vec![2, 5])
        .with_postings(postings.clone())
        .with_commit_ts(7)
        .with_splits(vec![1, 9])
        .with_uid_bytes(vec![0x02, 0x03]);
    assert_eq!(bl.uids(), &[2, 5]);
    assert_eq!(bl.postings(), &postings[..]);
    assert_eq!(bl.commit_ts(), 7);
    assert_eq!(bl.splits(), &[1, 9]);
    assert_eq!(bl.uid_bytes(), &[0x02, 0x03]);
}

#[test]
fn the_default_backup_list_is_empty() {
    let bl = BackupPostingList::default();
    assert!(bl.uids().is_empty() && bl.postings().is_empty() && bl.splits().is_empty());
    assert_eq!((bl.commit_ts(), bl.uid_bytes()), (0, &[][..]));
}
