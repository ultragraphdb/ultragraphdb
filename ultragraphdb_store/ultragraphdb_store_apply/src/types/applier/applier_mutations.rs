/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, worker/draft.go (applyMutations) and worker/mutation.go (runTypeMutation,
 * updateType). Changed for ultragraphdb: translated to Rust; without Raft, a proposal's mutations are applied as one
 * request committed on its own; pending transactions are not checked, a request that read before a drop or schema
 * change failing at its commit instead.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::sync::Arc;

use ultragraphdb_common_utils::go_strconv;
use ultragraphdb_core_schema::type_entry;
use ultragraphdb_proto_pb::pb::{Mutations, TypeUpdate, mutations::DropOp};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_store::{Executor, ListCache, Store};

use crate::errors::apply_error::ApplyError;
use crate::types::applier::Applier;
use crate::utils::lock::lock;
use crate::utils::validate::{is_delete_predicate_edge, type_sanity_check};

impl<S, C, E> Applier<S, C, E>
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    E: Executor + Send + Sync + 'static,
{
    /// Dgraph's `applyMutations`: a drop, a schema and type change, a predicate's drop given as the edge `* <p> *`,
    /// or edges applied in a request reading at the mutations' start timestamp and committed on its own. Returns the
    /// drop's, the change's or the commit's timestamp.
    ///
    /// # Errors
    ///
    /// A drop value that is not a namespace, mutations without a start timestamp, and the errors of the operation.
    pub fn apply_mutations(self: &Arc<Self>, m: &Mutations) -> Result<u64, ApplyError> {
        match DropOp::try_from(m.drop_op).unwrap_or(DropOp::None) {
            DropOp::AllInNs => {
                return self.drop_all_in_ns(go_strconv::parse_uint(&m.drop_value, 0)?);
            }
            DropOp::Data => return self.drop_data(go_strconv::parse_uint(&m.drop_value, 0)?),
            DropOp::All => return self.drop_all(),
            DropOp::Type => return self.drop_type(&m.drop_value),
            DropOp::None => {}
        }
        if m.start_ts == 0 {
            return Err(ApplyError::StartTsRequired());
        }
        if !m.schema.is_empty() || !m.types.is_empty() {
            let mut ts = 0;
            if !m.schema.is_empty() {
                ts = self.alter(m.schema.clone())?;
            }
            if !m.types.is_empty() {
                ts = self.update_types(m.types.clone())?;
            }
            return Ok(ts);
        }
        if let Some(edge) = m.edges.iter().find(|e| is_delete_predicate_edge(e)) {
            return self.drop_predicate(&edge.attr);
        }
        let mut batch = self.batch();
        let mut request = batch.request(m.start_ts);
        request.apply_edges(m)?;
        batch.push(request);
        first_result(batch.commit()?)
    }

    /// Dgraph's `runTypeMutation`: sets each type and writes it, in one batch: the batch's commit timestamp.
    ///
    /// # Errors
    ///
    /// A type `typeSanityCheck` refuses, and the store's error.
    pub fn update_types(&self, types: Vec<TypeUpdate>) -> Result<u64, ApplyError> {
        types.iter().try_for_each(type_sanity_check)?;
        let _apply = lock(&self.apply);
        let mut request = self.lists.request(self.lists.latest_ts());
        for t in &types {
            request.add_entry(type_entry(t, 0)?);
        }
        let mut batch = self.lists.batch();
        batch.push(request);
        let results = self.lists.commit_with(batch, |_| {
            for t in &types {
                self.schema.set_type(&t.type_name, t.clone());
            }
            Ok::<_, ApplyError>(())
        })?;
        first_result(results)
    }
}

/// The result of the one request of a batch.
pub(crate) fn first_result(results: Vec<Result<u64, ApplyError>>) -> Result<u64, ApplyError> {
    results
        .into_iter()
        .next()
        .unwrap_or_else(|| Err(ApplyError::StartTsRequired()))
}
