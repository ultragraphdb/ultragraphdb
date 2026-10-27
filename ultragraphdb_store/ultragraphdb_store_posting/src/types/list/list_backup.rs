/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/list.go (ToBackupPostingList, FromBackupPostingList). Changed for
 * ultragraphdb: translated to Rust; the entry is a store entry, whose value is the rolled-up list in the posting
 * format, where Dgraph's is the backup list's protobuf encoding; Dgraph's assertion that the rollup at the maximum
 * timestamp has an output holds by construction, and the port returns `None` rather than asserting.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_codec::{
    BackupPostingList, PostingList, decode_to_buffer, encode, encode_from_buffer,
};
use ultragraphdb_traits_store::Entry;

use crate::errors::posting_error::PostingError;
use crate::types::list::List;
use crate::utils::constants::{BIT_COMPLETE_POSTING, BIT_EMPTY_POSTING, BLOCK_SIZE};
use crate::utils::postings::is_plist_empty;

impl List {
    /// The list rolled up at the maximum timestamp into one unsplit list in the backup format, and its entry at the
    /// rollup's version, holding the rolled-up list: Dgraph's `ToBackupPostingList`.
    pub fn to_backup_posting_list(
        &self,
    ) -> Result<Option<(BackupPostingList, Entry)>, PostingError> {
        let Some(out) = self.rollup(u64::MAX, false, usize::MAX)? else {
            return Ok(None);
        };
        let ol = out.plist();
        let mut uid_bytes = Vec::new();
        if let Some(pack) = ol.pack() {
            decode_to_buffer(&mut uid_bytes, pack);
        }
        let bl = BackupPostingList::default()
            .with_uid_bytes(uid_bytes)
            .with_postings(ol.postings().to_vec())
            .with_commit_ts(ol.commit_ts())
            .with_splits(ol.splits().to_vec());
        let meta = if is_plist_empty(ol) {
            BIT_EMPTY_POSTING
        } else {
            BIT_COMPLETE_POSTING
        };
        let entry = Entry::put(self.key.clone(), out.new_min_ts(), meta, ol.encode());
        Ok(Some((bl, entry)))
    }
}

/// The posting list a backup's list stands for: Dgraph's `FromBackupPostingList`.
///
/// # Errors
///
/// The codec's error for uid bytes that are not uvarint deltas, where Dgraph panics or never returns.
pub fn from_backup_posting_list(bl: &BackupPostingList) -> Result<PostingList, PostingError> {
    let pack = if !bl.uids().is_empty() {
        encode(bl.uids(), BLOCK_SIZE)
    } else if !bl.uid_bytes().is_empty() {
        encode_from_buffer(bl.uid_bytes(), BLOCK_SIZE)
            .map_err(|e| PostingError::corrupted(b"", e.to_string()))?
    } else {
        None
    };
    let mut plist = PostingList::default()
        .with_postings(bl.postings().to_vec())
        .with_commit_ts(bl.commit_ts())
        .with_splits(bl.splits().to_vec());
    plist.set_pack(pack);
    Ok(plist)
}
