/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/mvcc.go (ReadPostingList) and posting/list.go (readListPart). Changed for
 * ultragraphdb: translated to Rust; a list reads its versions through a store snapshot, and a split list's parts
 * with them, through the same snapshot; an empty version is the list's complete version, as Dgraph's
 * `RunWithoutTemp` treats it, so a read below it is refused rather than served the empty list (`mvcc.go:708-709`);
 * every value is checked as it is decoded.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::sync::Arc;

use crate::utils::list_keys::{is_split_key, split_key_of};
use ultragraphdb_error_store::StoreError;
use ultragraphdb_proto_pb::pb::PostingList;
use ultragraphdb_traits_store::{Item, StoreSnapshot};

use crate::errors::posting_error::PostingError;
use crate::types::list::List;
use crate::utils::constants::{
    BIT_COMPLETE_POSTING, BIT_DELTA_POSTING, BIT_EMPTY_POSTING, BIT_SCHEMA_POSTING,
};
use crate::utils::posting_codec::{unmarshal_complete, unmarshal_delta};

impl List {
    /// The list of `key` from its versions, newest first, as a store snapshot reports them: Dgraph's
    /// `ReadPostingList`. The deltas are read down to the newest complete or empty version, a version carrying the
    /// discard flag, or the newest deletion. Returns the list and how many deltas it read, which decides whether the
    /// key is rolled up.
    ///
    /// # Errors
    ///
    /// [`PostingError::InvalidKey`] for a split part's key; [`PostingError::SchemaPosting`] and
    /// [`PostingError::UnexpectedMeta`] for a version that is not a posting list; the store's error, and its
    /// `Corrupted` for a value that does not decode.
    pub fn read_versions<I>(key: &[u8], versions: I) -> Result<(List, usize), PostingError>
    where
        I: IntoIterator<Item = Result<Item, StoreError>>,
    {
        if is_split_key(key)? {
            return Err(PostingError::InvalidKey());
        }
        let mut list = List::empty(key.to_vec());
        let mut deltas = 0;
        for item in versions {
            let item = item?;
            if item.key() != key {
                break;
            }
            list.max_ts = list.max_ts.max(item.version());
            match item.meta() {
                BIT_EMPTY_POSTING => {
                    list.min_ts = item.version();
                    return Ok((list, deltas));
                }
                BIT_COMPLETE_POSTING => {
                    list.plist = Arc::new(unmarshal_complete(key, item.value())?);
                    list.min_ts = item.version();
                    return Ok((list, deltas));
                }
                BIT_DELTA_POSTING => {
                    let mut delta = unmarshal_delta(key, item.value())?;
                    delta.commit_ts = item.version();
                    list.mutation.insert_committed_postings(delta);
                    deltas += 1;
                }
                BIT_SCHEMA_POSTING => return Err(PostingError::SchemaPosting(key.to_vec())),
                meta => return Err(PostingError::UnexpectedMeta(meta, key.to_vec())),
            }
            if item.discard() {
                break;
            }
        }
        Ok((list, deltas))
    }

    /// The list of `key` at the snapshot's read timestamp, the parts of a split list read through the same
    /// snapshot, and how many deltas it read.
    pub fn read<S: StoreSnapshot>(key: &[u8], snapshot: &S) -> Result<(List, usize), PostingError> {
        let (mut list, deltas) = Self::read_versions(key, snapshot.versions(key))?;
        if !list.plist.splits.is_empty() {
            let keys = list
                .plist
                .splits
                .iter()
                .map(|&start| split_key_of(key, start))
                .collect::<Result<Vec<_>, _>>()?;
            let items = snapshot.get_many(&keys)?;
            let parts = keys
                .iter()
                .zip(items)
                .map(|(part_key, item)| read_part(part_key, item))
                .collect::<Result<Vec<_>, _>>()?;
            list = list.with_parts(parts);
        }
        list.mutation.set_ts(snapshot.read_ts());
        Ok((list, deltas))
    }
}

/// One part of a split list as the store holds it.
fn read_part(key: &[u8], item: Option<Item>) -> Result<PostingList, PostingError> {
    let item = item.ok_or_else(|| PostingError::MissingPart(key.to_vec()))?;
    match item.meta() {
        BIT_COMPLETE_POSTING => unmarshal_complete(key, item.value()),
        BIT_EMPTY_POSTING => Ok(PostingList::default()),
        meta => Err(PostingError::UnexpectedMeta(meta, key.to_vec())),
    }
}
