/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/list.go (List, NewList). Changed for ultragraphdb: translated to Rust; a list
 * is a value without a lock, its immutable parts shared through `Arc`s, so the cache holds immutable lists and a
 * request owns its copy; a split list holds its parts, read with the main key through one snapshot, where Dgraph
 * reads each part from Badger when an iteration reaches it.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod list_backup;
pub use list_backup::from_backup_posting_list;
mod list_commit;
mod list_iterate;
mod list_mutation;
mod list_read;
mod list_rollup;
mod list_uids;
mod list_values;

use std::sync::Arc;

use ultragraphdb_proto_pb::pb::PostingList;

use crate::types::mutable_layer::MutableLayer;
use crate::utils::list_keys::is_quad_list;

/// A posting list as of the versions read: Dgraph's `List`. Its immutable layer is the newest complete version,
/// written at [`List::min_ts`], with the parts of a split list; above it, the [`MutableLayer`] holds the committed
/// deltas and the scratch layer of the request reading it.
///
/// A list is a value: cloning it shares the immutable layer and the committed deltas and copies the scratch
/// layer, so the cache holds a list and every request reads its own copy.
#[derive(Debug, Clone, PartialEq)]
pub struct List {
    key: Vec<u8>,
    plist: Arc<PostingList>,
    parts: Arc<Vec<Arc<PostingList>>>,
    mutation: MutableLayer,
    min_ts: u64,
    max_ts: u64,
}

impl List {
    /// A list whose immutable layer is `plist`, complete at `min_ts`, with no deltas: Dgraph's `NewList`. A split
    /// `plist` needs its parts, from [`List::with_parts`].
    pub fn new(key: Vec<u8>, plist: PostingList, min_ts: u64) -> Self {
        let mut mutation = MutableLayer::new();
        mutation.set_by_value(is_quad_list(&key));
        Self {
            key,
            plist: Arc::new(plist),
            parts: Arc::default(),
            mutation,
            min_ts,
            max_ts: min_ts,
        }
    }

    /// The list with the parts its immutable layer's splits name, in their order.
    pub fn with_parts(mut self, parts: Vec<PostingList>) -> Self {
        self.parts = Arc::new(parts.into_iter().map(Arc::new).collect());
        self
    }

    /// An empty list of `key` with no versions, which a request writes without reading: what Dgraph's
    /// `GetFromDelta` builds.
    pub fn empty(key: Vec<u8>) -> Self {
        Self::new(key, PostingList::default(), 0)
    }

    pub fn key(&self) -> &[u8] {
        &self.key
    }

    /// The commit timestamp of the immutable layer; a read below it is refused.
    pub fn min_ts(&self) -> u64 {
        self.min_ts
    }

    /// The newest version read for the list.
    pub fn max_ts(&self) -> u64 {
        self.max_ts
    }

    /// The immutable layer: the complete version, or for a split list its splits.
    pub fn plist(&self) -> &PostingList {
        &self.plist
    }

    /// The parts of a split list, in the order of its splits.
    pub fn parts(&self) -> &[Arc<PostingList>] {
        &self.parts
    }

    /// The start uid of each part, empty for a list that is not split: Dgraph's `PartSplits`.
    pub fn part_splits(&self) -> Vec<u64> {
        self.plist.splits.clone()
    }

    pub fn mutation(&self) -> &MutableLayer {
        &self.mutation
    }

    pub(crate) fn mutation_mut(&mut self) -> &mut MutableLayer {
        &mut self.mutation
    }

    /// Sets the read timestamp of the request reading the list: Dgraph's `SetTs`.
    pub fn set_ts(&mut self, read_ts: u64) {
        self.mutation.set_ts(read_ts);
    }

    /// The approximate number of uids: the deltas plus the immutable layer's blocks times their size, Dgraph's
    /// `ApproxLen`.
    pub fn approx_len(&self) -> usize {
        let immutable: usize = self
            .immutable_parts()
            .iter()
            .filter_map(|p| p.pack.as_ref())
            .map(ultragraphdb_core_codec::approx_len)
            .sum();
        self.mutation.len() + immutable
    }

    /// The posting lists of the immutable layer in uid order: the parts of a split list, or the complete list.
    pub(crate) fn immutable_parts(&self) -> Vec<&PostingList> {
        if self.plist.splits.is_empty() {
            vec![self.plist.as_ref()]
        } else {
            self.parts.iter().map(Arc::as_ref).collect()
        }
    }
}
