/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::{BackupPostingList, Posting};
use ultragraphdb_tool_bdd::TestResult;

use crate::codec_tests::{id, sequences};

#[test]
fn test_a_backup_list_returns_what_it_was_built_with() -> TestResult {
    let postings = vec![Posting::default().with_uid(4)];
    let uids = sequences([2, 5].into_iter())?;
    let splits = vec![1, id(1, 0, 9)?.to_u128()];
    let bl = BackupPostingList::default()
        .with_uids(uids.clone())
        .with_postings(postings.clone())
        .with_commit_ts(7)
        .with_splits(splits.clone())
        .with_uid_bytes(vec![0x02, 0x03]);
    assert_eq!(bl.uids(), &uids[..]);
    assert_eq!(bl.postings(), &postings[..]);
    assert_eq!(bl.commit_ts(), 7);
    assert_eq!(bl.splits(), &splits[..]);
    assert_eq!(bl.uid_bytes(), &[0x02, 0x03]);
    Ok(())
}

#[test]
fn test_the_default_backup_list_is_empty() {
    let bl = BackupPostingList::default();
    assert!(bl.uids().is_empty() && bl.postings().is_empty() && bl.splits().is_empty());
    assert_eq!((bl.commit_ts(), bl.uid_bytes()), (0, &[][..]));
}
