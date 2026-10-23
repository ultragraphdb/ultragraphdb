/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/list.go (MutableLayer). Changed for ultragraphdb: translated to Rust; the
 * committed deltas are an immutable map shared by every copy of a list, a request's scratch layer is visible to
 * reads at the request's read timestamp alone, and the writes of the requests applied before it in its batch form
 * stacked layers between the two; the caches Dgraph keeps beside the deltas (committedUids, length, the delete-all
 * marker, the calculated uids) are computed when asked, so no cache can disagree with the deltas.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod mutable_layer_insert;

use std::collections::BTreeMap;
use std::sync::Arc;

use ultragraphdb_proto_pb::pb::{Posting, PostingList};

use crate::utils::constants::DEL;
use crate::utils::postings::has_delete_all;

/// The deltas above a posting list's complete version: Dgraph's `MutableLayer`.
///
/// It holds three kinds of layer, from oldest to newest: the committed deltas, by commit timestamp; the stacked
/// layers, the writes of the requests applied before this one in its batch, in their order; and the current
/// entries, the scratch layer of the request reading at [`MutableLayer::read_ts`]. A read at a timestamp sees the
/// committed deltas at or below it, and the stacked layers and the current entries only at the request's own read
/// timestamp.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MutableLayer {
    committed: Arc<BTreeMap<u64, Arc<PostingList>>>,
    stacked: Vec<Arc<PostingList>>,
    current: Option<PostingList>,
    /// The index of each uid's posting in the current entries, Dgraph's `currentUids`.
    current_uids: BTreeMap<u64, usize>,
    read_ts: u64,
}

/// Where a layer sits among the visible layers, oldest first: a committed delta by its commit timestamp, a stacked
/// layer by its place in the batch, then the current entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Rank {
    Committed(u64),
    Stacked(usize),
    Current,
}

impl MutableLayer {
    pub fn new() -> Self {
        Self::default()
    }

    /// The request's read timestamp, at which its stacked layers and current entries are visible.
    pub fn read_ts(&self) -> u64 {
        self.read_ts
    }

    /// Sets the read timestamp: Dgraph's `setTs`.
    pub fn set_ts(&mut self, read_ts: u64) {
        self.read_ts = read_ts;
    }

    /// The scratch layer of the request, if it wrote anything.
    pub fn current_entries(&self) -> Option<&PostingList> {
        self.current.as_ref()
    }

    /// Replaces the scratch layer with `plist` at read timestamp `ts`: Dgraph's `setCurrentEntries`.
    pub fn set_current_entries(&mut self, ts: u64, plist: PostingList) {
        self.read_ts = ts;
        self.current_uids = uid_index(&plist);
        self.current = Some(plist);
    }

    /// The stacked layers, oldest first.
    pub fn stacked(&self) -> &[Arc<PostingList>] {
        &self.stacked
    }

    pub(crate) fn set_stacked(&mut self, stacked: Vec<Arc<PostingList>>) {
        self.stacked = stacked;
    }

    /// The committed deltas by commit timestamp.
    pub fn committed(&self) -> &BTreeMap<u64, Arc<PostingList>> {
        &self.committed
    }

    /// Adds a committed delta read from the store: Dgraph's `insertCommittedPostings`. The delta's commit timestamp
    /// is its `commit_ts`, which every posting of it takes.
    pub fn insert_committed_postings(&mut self, mut plist: PostingList) {
        let commit_ts = plist.commit_ts;
        plist
            .postings
            .iter_mut()
            .for_each(|p| p.commit_ts = commit_ts);
        Arc::make_mut(&mut self.committed).insert(commit_ts, Arc::new(plist));
    }

    /// The number of layers: committed deltas, stacked layers and the current entries. Dgraph's `len`: whether the
    /// layer holds anything and roughly how much.
    pub fn len(&self) -> usize {
        self.committed.len() + self.stacked.len() + usize::from(self.current.is_some())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The newest commit timestamp among the committed deltas, `u64::MAX` when there are none: Dgraph's
    /// `committedUidsTime`.
    pub fn committed_uids_time(&self) -> u64 {
        self.committed
            .keys()
            .next_back()
            .copied()
            .unwrap_or(u64::MAX)
    }

    /// The layers visible at `read_ts`, oldest first.
    pub(crate) fn visible(&self, read_ts: u64) -> Vec<(Rank, &PostingList)> {
        let own = read_ts == self.read_ts;
        let committed = self
            .committed
            .range(..=read_ts)
            .map(|(ts, pl)| (Rank::Committed(*ts), pl.as_ref()));
        let stacked = self
            .stacked
            .iter()
            .enumerate()
            .filter(move |_| own)
            .map(|(i, pl)| (Rank::Stacked(i), pl.as_ref()));
        let current = self
            .current
            .iter()
            .filter(move |_| own)
            .map(|pl| (Rank::Current, pl));
        committed.chain(stacked).chain(current).collect()
    }

    /// The newest visible layer at `read_ts` holding a delete-all, if any.
    pub(crate) fn delete_all_rank(&self, read_ts: u64) -> Option<Rank> {
        self.visible(read_ts)
            .into_iter()
            .rev()
            .find(|(_, pl)| pl.postings.iter().any(has_delete_all))
            .map(|(rank, _)| rank)
    }

    /// The timestamp of the newest delete-all visible at `read_ts`, 0 when there is none: Dgraph's
    /// `populateDeleteAll`. A delete-all of a stacked layer or of the current entries is at the read timestamp.
    pub fn populate_delete_all(&self, read_ts: u64) -> u64 {
        match self.delete_all_rank(read_ts) {
            Some(Rank::Committed(ts)) => ts,
            Some(_) => self.read_ts,
            None => 0,
        }
    }

    /// The visible layers at `read_ts` from the newest delete-all on, oldest first, and that delete-all's rank:
    /// Dgraph's `iterate`.
    pub(crate) fn live(&self, read_ts: u64) -> (Option<Rank>, Vec<(Rank, &PostingList)>) {
        let marker = self.delete_all_rank(read_ts);
        let layers = self
            .visible(read_ts)
            .into_iter()
            .filter(|(rank, _)| marker.is_none_or(|m| *rank >= m))
            .collect();
        (marker, layers)
    }

    /// What the visible layers add to a list's length at `read_ts`: Dgraph's `listLen`, which counts a set as one,
    /// a delete as minus one and an overwrite as nothing, from the newest delete-all on.
    pub fn list_len(&self, read_ts: u64) -> i64 {
        let (_, layers) = self.live(read_ts);
        layers
            .iter()
            .flat_map(|(_, pl)| pl.postings.iter())
            .filter(|p| !has_delete_all(p))
            .map(|p| crate::utils::postings::length_delta(p.op))
            .sum()
    }

    /// What the committed deltas alone add to a list's length, read above all of them: the length Dgraph caches in
    /// `MutableLayer.length`.
    pub fn committed_len(&self) -> i64 {
        // A layer without a scratch layer or stacked layers shows the committed deltas alone at any timestamp.
        self.committed_only().list_len(u64::MAX)
    }

    /// Whether the immutable layer still has to be searched for `uid` at `read_ts`, and the posting the visible
    /// layers hold for it: Dgraph's `findPosting`. A delete found, or a delete-all, ends the search with nothing.
    pub fn find_posting(&self, read_ts: u64, uid: u64) -> (bool, Option<&Posting>) {
        let (marker, layers) = self.live(read_ts);
        let found = layers
            .iter()
            .rev()
            .find_map(|(_, pl)| pl.postings.iter().rev().find(|p| p.uid == uid));
        match found {
            Some(p) if p.op != DEL => (false, Some(p)),
            Some(_) => (false, None),
            None => (marker.is_none(), None),
        }
    }

    /// Adds `delta` as the committed delta at `commit_ts`, every posting of it taking that commit timestamp, and
    /// drops the scratch layer and the stacked layers: Dgraph's `setMutationAfterCommit`. The deltas are copied
    /// only when another list shares them.
    pub(crate) fn commit(&mut self, commit_ts: u64, mut delta: PostingList) {
        delta.commit_ts = commit_ts;
        delta
            .postings
            .iter_mut()
            .for_each(|p| p.commit_ts = commit_ts);
        Arc::make_mut(&mut self.committed).insert(commit_ts, Arc::new(delta));
        self.stacked.clear();
        self.current = None;
        self.current_uids.clear();
        self.read_ts = 0;
    }

    /// The layer without its stacked layers and scratch layer, as a cache holds it.
    pub(crate) fn committed_only(&self) -> Self {
        Self {
            committed: self.committed.clone(),
            stacked: Vec::new(),
            current: None,
            current_uids: BTreeMap::new(),
            read_ts: 0,
        }
    }
}

/// The index of each uid's last posting in `plist`.
fn uid_index(plist: &PostingList) -> BTreeMap<u64, usize> {
    plist
        .postings
        .iter()
        .enumerate()
        .map(|(i, p)| (p.uid, i))
        .collect()
}
