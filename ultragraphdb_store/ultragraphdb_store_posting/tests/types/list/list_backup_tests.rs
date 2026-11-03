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
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::helpers::{add, del_uid, ids, key, set_uid, with_facet};

fn unhex(text: &str) -> TestResult<Vec<u8>> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2).context("two digits")?, 16).context("hex"))
        .collect()
}

#[test]
fn test_a_backup_holds_the_ids_as_leb128_deltas() -> TestResult {
    let mut l = List::empty(key("backup-bytes", 1));
    [1, 3, 300]
        .iter()
        .for_each(|&u| add(&mut l, 1, &set_uid(u)));
    add(&mut l, 1, &with_facet(set_uid(3), "f"));
    l.commit_mutation(6);
    let (bl, entry) = l.to_backup_posting_list()?.context("a backup list")?;
    // Written from the format, LEB128 of each ID's 128-bit value less the one before, the first from 0 (Python
    // `int`s): the ID numbered 1, 2^112 + 2^48 (17 bytes), then 2 × 2^48 (8 bytes) and 297 × 2^48 (9 bytes).
    let golden = unhex("808080808080c0808080808080808080018080808080808001808080808080c09401")?;
    assert_eq!(bl.uid_bytes(), golden);
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
    let plist = from_backup_posting_list(&bl)?;
    assert_eq!(
        decode(plist.pack().context("a pack")?, 0),
        ids(&[1, 3, 300])?
    );
    assert_eq!(plist.postings(), bl.postings());
    Ok(())
}

#[test]
fn test_an_emptied_list_backs_up_empty() -> TestResult {
    let mut l = List::empty(key("backup-empty", 1));
    add(&mut l, 1, &set_uid(4));
    l.commit_mutation(2);
    add(&mut l, 3, &del_uid(4));
    l.commit_mutation(4);
    let (bl, entry) = l.to_backup_posting_list()?.context("a backup list")?;
    assert!(bl.uid_bytes().is_empty() && bl.postings().is_empty());
    assert_eq!((entry.version(), entry.meta()), (4, BIT_EMPTY_POSTING));
    assert_eq!(from_backup_posting_list(&bl)?.pack(), None);
    Ok(())
}

#[test]
fn test_a_backup_of_uids_or_of_uid_bytes_reads_the_same_list() -> TestResult {
    let uids = BackupPostingList::default()
        .with_uids(ids(&[2, 5, 9])?)
        // Ignored when the uids are given.
        .with_uid_bytes(vec![0x07])
        .with_commit_ts(11)
        .with_splits(vec![0]);
    // Written from the format (Python `int`s): LEB128 of the ID numbered 2, 2^112 + 2 × 2^48, then of 3 × 2^48 and
    // 4 × 2^48.
    let bytes = BackupPostingList::default().with_uid_bytes(unhex(
        "8080808080808081808080808080808001808080808080c0018080808080808002",
    )?);
    let from_uids = from_backup_posting_list(&uids)?;
    let from_bytes = from_backup_posting_list(&bytes)?;
    assert_eq!(
        decode(from_uids.pack().context("a pack")?, 0),
        ids(&[2, 5, 9])?
    );
    assert_eq!(from_uids.pack(), from_bytes.pack());
    assert_eq!((from_uids.commit_ts(), from_uids.splits()), (11, &[0][..]));
    Ok(())
}

#[test]
fn test_uid_bytes_that_end_inside_a_varint_are_refused() -> TestResult {
    // Deviation: Dgraph's loop never ends on a truncated uvarint; the port refuses it as a corrupted value. The ID
    // numbered 1 as LEB128, then a byte promising another group.
    let mut cut = unhex("808080808080c080808080808080808001")?;
    cut.push(0x80);
    let truncated = BackupPostingList::default().with_uid_bytes(cut);
    let key = match from_backup_posting_list(&truncated) {
        Err(PostingError(PostingErrorEnum::Store(StoreError(StoreErrorEnum::Corrupted {
            key,
            ..
        })))) => Ok(key),
        other => Err(format!("not refused: {other:?}")),
    }
    .context("the truncated uid bytes are refused as corrupted")?;
    assert!(key.is_empty(), "a backup's list names no key");
    Ok(())
}
