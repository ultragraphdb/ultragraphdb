/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/list.go (ToBackupPostingList, FromBackupPostingList). Changed for
 * ultragraphdb: translated to Rust; the entry is a store entry; Dgraph's assertion that the rollup at the maximum
 * timestamp has an output holds by construction, and the port returns `None` rather than asserting.
 * Copyright (c) 2026. Marvin Hansen.
 */

use prost::Message;
use ultragraphdb_core_codec::{decode_to_buffer, encode, encode_from_buffer};
use ultragraphdb_proto_pb::pb::{BackupPostingList, PostingList};
use ultragraphdb_traits_store::Entry;

use crate::errors::posting_error::PostingError;
use crate::types::list::List;
use crate::utils::constants::{BIT_COMPLETE_POSTING, BIT_EMPTY_POSTING, BLOCK_SIZE};
use crate::utils::postings::is_plist_empty;

impl List {
    /// The list rolled up at the maximum timestamp into one unsplit list in the backup format, and its entry at the
    /// rollup's version: Dgraph's `ToBackupPostingList`.
    pub fn to_backup_posting_list(
        &self,
    ) -> Result<Option<(BackupPostingList, Entry)>, PostingError> {
        let Some(out) = self.rollup(u64::MAX, false, usize::MAX)? else {
            return Ok(None);
        };
        let ol = out.plist();
        let mut uid_bytes = Vec::new();
        if let Some(pack) = &ol.pack {
            decode_to_buffer(&mut uid_bytes, pack);
        }
        let bl = BackupPostingList {
            uid_bytes,
            postings: ol.postings.clone(),
            commit_ts: ol.commit_ts,
            splits: ol.splits.clone(),
            ..BackupPostingList::default()
        };
        let meta = if is_plist_empty(ol) {
            BIT_EMPTY_POSTING
        } else {
            BIT_COMPLETE_POSTING
        };
        let entry = Entry::put(self.key.clone(), out.new_min_ts(), meta, bl.encode_to_vec());
        Ok(Some((bl, entry)))
    }
}

/// The posting list a backup's list stands for: Dgraph's `FromBackupPostingList`.
///
/// # Errors
///
/// The codec's error for uid bytes that are not uvarint deltas, where Dgraph panics or never returns.
pub fn from_backup_posting_list(bl: &BackupPostingList) -> Result<PostingList, PostingError> {
    let pack = if !bl.uids.is_empty() {
        encode(&bl.uids, BLOCK_SIZE)
    } else if !bl.uid_bytes.is_empty() {
        encode_from_buffer(&bl.uid_bytes, BLOCK_SIZE)
            .map_err(|e| PostingError::corrupted(b"", e.to_string()))?
    } else {
        None
    };
    Ok(PostingList {
        pack,
        postings: bl.postings.clone(),
        commit_ts: bl.commit_ts,
        splits: bl.splits.clone(),
    })
}
