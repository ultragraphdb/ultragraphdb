/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

use ultragraphdb_proto_pb::pb::SchemaUpdate;
use ultragraphdb_store_posting::{Batch, CachedList};
use ultragraphdb_traits_store::{Executor, ListCache, Store};

use crate::errors::apply_error::ApplyError;
use crate::types::applier::Applier;
use crate::types::apply_request::ApplyRequest;

/// Requests that apply edges, committed together in one store batch, each at its own commit timestamp. A request
/// begun from the batch reads the writes of those pushed before it.
#[derive(Debug)]
pub struct ApplyBatch<'a, S, C, E> {
    applier: &'a Applier<S, C, E>,
    batch: Batch<'a, S, C>,
    created: Vec<BTreeMap<String, SchemaUpdate>>,
}

impl<'a, S, C, E> ApplyBatch<'a, S, C, E>
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    E: Executor + Send + Sync + 'static,
{
    pub(crate) fn new(applier: &'a Applier<S, C, E>, batch: Batch<'a, S, C>) -> Self {
        Self {
            applier,
            batch,
            created: Vec::new(),
        }
    }

    /// A request reading at `read_ts` that sees the writes of the requests pushed so far.
    pub fn request(&self, read_ts: u64) -> ApplyRequest<'a, S, C, E> {
        ApplyRequest::new(self.applier, self.batch.request(read_ts))
    }

    /// Adds `request` to the batch: its writes stack under the requests begun after it.
    pub fn push(&mut self, request: ApplyRequest<'a, S, C, E>) {
        let (request, created) = request.into_parts();
        self.batch.push(request);
        self.created.push(created);
    }

    pub fn len(&self) -> usize {
        self.batch.len()
    }

    pub fn is_empty(&self) -> bool {
        self.batch.is_empty()
    }

    /// Commits every request of the batch, in order, each at its commit timestamp, in one store batch. A request that
    /// read below a drop or schema change it writes under is refused, and so is every later request that read a list
    /// a refused one wrote; a request whose new predicate gained a schema since its read has its values converted to
    /// that schema, or is refused with the conversion's error. A refused request writes nothing.
    ///
    /// # Errors
    ///
    /// The store's error when the batch cannot be written.
    pub fn commit(self) -> Result<Vec<Result<u64, ApplyError>>, ApplyError> {
        self.applier.commit_batch(self.batch, self.created)
    }
}
