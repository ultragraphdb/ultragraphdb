/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/list.go (addMutation, addMutationInternal, updateMutationLayer). Changed for
 * ultragraphdb: translated to Rust; the predicate's schema is the caller's, where Dgraph asks its global schema;
 * the edge is not changed, where Dgraph writes the value's fingerprint into its ValueId; conflict keys, the abort
 * check and the vector index's value cache are not ported here.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_keys::parse;
use ultragraphdb_proto_pb::pb::{DirectedEdge, Posting, PostingList, SchemaUpdate, posting};

use crate::errors::posting_error::PostingError;
use crate::types::list::List;
use crate::utils::constants::DEL;
use crate::utils::postings::{delete_all_posting, fingerprint_edge, has_delete_all, new_posting};

impl List {
    /// Applies `edge` to the list's scratch layer at the list's read timestamp: Dgraph's `addMutation`. `schema` is
    /// the edge's predicate as the request's schema holds it, `None` when it has none. A value's posting is
    /// identified by [`fingerprint_edge`]; a set on a predicate of a single uid replaces the uid it held; with a
    /// count index, a delete of a uid set by this request removes that set.
    ///
    /// # Errors
    ///
    /// [`PostingError::UnhandledOperation`] for an unknown operation, and a key that does not parse.
    pub fn add_mutation(
        &mut self,
        edge: &DirectedEdge,
        schema: Option<&SchemaUpdate>,
    ) -> Result<(), PostingError> {
        let mut mpost = new_posting(edge)?;
        mpost.start_ts = self.mutation.read_ts();
        if mpost.posting_type != posting::PostingType::Ref as i32 {
            mpost.uid = fingerprint_edge(edge, schema.is_some_and(|s| s.list));
        }
        let pk = parse(&self.key)?;
        let single_uid_update = schema
            .is_some_and(|s| !s.list && s.value_type == posting::ValType::Uid as i32)
            && pk.is_data()
            && mpost.op != DEL
            && mpost.posting_type == posting::PostingType::Ref as i32;
        let has_count_index = schema.is_some_and(|s| s.count) && (pk.is_data() || pk.is_reverse());
        self.update_mutation_layer(mpost, single_uid_update, has_count_index);
        Ok(())
    }

    /// Adds `posting` to the list's scratch layer at the list's read timestamp, as built by its caller: what a quad
    /// list's writes use, whose postings carry a uid, a literal's encoding and fingerprint, and statement IDs that
    /// the posting crate does not derive. A posting replaces the one of its identity, its uid and for a value posting
    /// its value; with `has_count_index`, a delete of a posting this scratch layer set removes the set.
    pub fn add_posting(&mut self, mut posting: Posting, has_count_index: bool) {
        posting.start_ts = self.mutation.read_ts();
        self.update_mutation_layer(posting, false, has_count_index);
    }

    /// Dgraph's `updateMutationLayer`: a delete-all replaces the scratch layer; a set of a single-uid predicate
    /// replaces it with a delete-all and the set; anything else goes in beside what is there.
    fn update_mutation_layer(
        &mut self,
        mpost: Posting,
        single_uid_update: bool,
        has_count_index: bool,
    ) {
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
        if single_uid_update {
            // The uid predicate's fingerprint is its target, not u64::MAX as for a scalar value, so the target it
            // held is removed by a delete-all.
            let mut postings = Vec::with_capacity(2);
            if mpost.op != DEL {
                postings.push(delete_all_posting());
            }
            postings.push(mpost);
            self.mutation.set_current_entries(
                start_ts,
                PostingList {
                    postings,
                    ..PostingList::default()
                },
            );
            return;
        }
        self.mutation.insert_posting(mpost, has_count_index);
    }
}
