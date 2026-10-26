/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/list.go (addMutationInternal, updateMutationLayer). Changed for ultragraphdb:
 * translated to Rust; a posting is built by its caller, which knows its identity, where Dgraph derives it from an
 * edge and its predicate's schema; conflict keys, the abort check and the vector index's value cache are not ported
 * here.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_proto_pb::pb::{Posting, PostingList};

use crate::types::list::List;
use crate::utils::postings::has_delete_all;

impl List {
    /// Adds `posting` to the list's scratch layer at the list's read timestamp, as built by its caller: what a quad
    /// list's writes use, whose postings carry a uid, a literal's encoding and fingerprint, and statement IDs that
    /// the posting crate does not derive. A posting replaces the one of its identity, its uid and for a value posting
    /// its value; with `has_count_index`, a delete of a posting this scratch layer set removes the set.
    pub fn add_posting(&mut self, mut posting: Posting, has_count_index: bool) {
        posting.start_ts = self.mutation.read_ts();
        self.update_mutation_layer(posting, has_count_index);
    }

    /// Dgraph's `updateMutationLayer`: a delete-all replaces the scratch layer; anything else goes in beside what is
    /// there.
    fn update_mutation_layer(&mut self, mpost: Posting, has_count_index: bool) {
        let start_ts = mpost.start_ts;
        if has_delete_all(&mpost) {
            self.mutation.set_current_entries(
                start_ts,
                PostingList {
                    postings: vec![mpost],
                    ..PostingList::default()
                },
            );
            return;
        }
        self.mutation.insert_posting(mpost, has_count_index);
    }
}
