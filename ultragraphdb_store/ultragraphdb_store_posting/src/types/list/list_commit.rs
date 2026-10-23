/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/list.go (getMutation, setMutationAfterCommit, setMutation). Changed for
 * ultragraphdb: translated to Rust; a commit makes a new list, sharing the old one's deltas, where Dgraph updates a
 * list in place (and copies its maps when `refresh` is set); Dgraph's assertion that a delta carries no pack is the
 * decoder's check of every stored delta.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_proto_pb::pb::PostingList;

use crate::types::list::List;
use crate::utils::posting_codec::marshal;

impl List {
    /// The request's scratch layer, as its delta: Dgraph's `GetPosting` at the read timestamp.
    pub fn delta(&self) -> Option<&PostingList> {
        self.mutation.current_entries()
    }

    /// The bytes of the request's delta, empty when the request wrote nothing to the list: Dgraph's `getMutation`.
    pub fn marshal_delta(&self) -> Vec<u8> {
        self.delta().map(marshal).unwrap_or_default()
    }

    /// The list with `delta` committed at `commit_ts` above its deltas, and no scratch layer: Dgraph's
    /// `setMutationAfterCommit`. A commit updates a cached list with it.
    pub fn committed(&self, commit_ts: u64, delta: PostingList) -> List {
        let mut list = self.clone();
        list.commit(commit_ts, delta);
        list
    }

    /// Commits `delta` at `commit_ts` to this list in place, as [`List::committed`] does.
    pub fn commit(&mut self, commit_ts: u64, delta: PostingList) {
        self.mutation.commit(commit_ts, delta);
        if commit_ts != 0 {
            self.max_ts = self.max_ts.max(commit_ts);
        }
    }

    /// Commits the list's own scratch layer at `commit_ts` in place: what Dgraph's tests do with `commitMutation`.
    pub fn commit_mutation(&mut self, commit_ts: u64) {
        let delta = self.delta().cloned().unwrap_or_default();
        self.commit(commit_ts, delta);
    }

    /// The list as a cache holds it: without a scratch layer or the writes of a batch.
    pub fn without_scratch(&self) -> List {
        let mut list = self.clone();
        list.mutation = self.mutation.committed_only();
        list
    }

    /// The list reading at `read_ts` with `delta` as its scratch layer, the writes a request made before it read
    /// the list: Dgraph's `setMutation`.
    pub fn with_scratch(mut self, read_ts: u64, delta: Option<PostingList>) -> List {
        match delta {
            Some(delta) => self.mutation.set_current_entries(read_ts, delta),
            None => self.mutation.set_ts(read_ts),
        }
        self
    }
}
