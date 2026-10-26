/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, worker/draft.go (applyMutations, without Raft) and worker/mutation.go
 * (runSchemaMutation). Changed for ultragraphdb: translated to Rust; one applier holds the posting lists, the schema
 * state and the executor of a store, where Dgraph keeps them in package globals; its commits, schema changes and
 * drops are serialised by one lock, as Dgraph's apply loop applies its proposals one at a time.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod applier_alter;
mod applier_commit;
mod applier_drop;
mod applier_mutations;
mod applier_rebuild;
mod applier_rebuild_record;
pub(crate) mod state;

use std::collections::BTreeMap;
use std::sync::Mutex;

use ultragraphdb_core_catalog::SchemaState;
use ultragraphdb_proto_pb::pb::SchemaUpdate;
use ultragraphdb_store_posting::{CachedList, Lists};
use ultragraphdb_traits_store::{Executor, ListCache, Store};

use crate::errors::apply_error::ApplyError;
use crate::types::applier::state::{ApplyState, Fence, Rebuilding};
use crate::types::apply_batch::ApplyBatch;
use crate::types::index_schema::IndexSchema;
use crate::types::level_draw::LevelDraw;
use crate::utils::lock::lock;

/// Applies edges, schema changes and drops to the posting lists of one store: Dgraph's `applyMutations` without
/// Raft. Edges are applied in requests of the local commit and committed in batches; a schema change that adds an
/// index builds it on the executor while writes maintain both schemas' indexes, and swaps it in with one batch; drops
/// remove what they drop physically.
///
/// **Ordering.** Commits, schema changes and drops are serialised by one lock: Dgraph's apply loop. Edges are applied
/// to their requests outside it, and a request whose read timestamp is below a drop or schema change it writes under
/// is refused at commit with `Transaction has been aborted. Please retry`.
#[derive(Debug)]
pub struct Applier<S, C, E> {
    lists: Lists<S, C>,
    schema: SchemaState,
    executor: E,
    draw: Mutex<LevelDraw>,
    apply: Mutex<()>,
    state: Mutex<ApplyState>,
}

impl<S, C, E> Applier<S, C, E>
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    E: Executor + Send + Sync + 'static,
{
    /// The applier of `lists`, its schema loaded from their store, its background work on `executor`, its vector
    /// index's level draws seeded by `seed`. A rebuild a crash or a power loss stopped is finished first: its record's
    /// prefixes the loaded schema does not declare are dropped.
    ///
    /// # Errors
    ///
    /// The schema's load error, and the store's error finishing a rebuild.
    pub fn new(lists: Lists<S, C>, executor: E, seed: u64) -> Result<Self, ApplyError> {
        let schema = SchemaState::new();
        schema.load_from_db(lists.store())?;
        let applier = Self {
            lists,
            schema,
            executor,
            draw: Mutex::new(LevelDraw::new(seed)),
            apply: Mutex::new(()),
            state: Mutex::new(ApplyState::default()),
        };
        applier.finish_rebuilds()?;
        Ok(applier)
    }

    pub fn lists(&self) -> &Lists<S, C> {
        &self.lists
    }

    /// The schema state: the current definitions, and while an index is built, the schema being applied.
    pub fn schema(&self) -> &SchemaState {
        &self.schema
    }

    pub fn executor(&self) -> &E {
        &self.executor
    }

    /// A batch of requests to apply edges in and commit together.
    pub fn batch(&self) -> ApplyBatch<'_, S, C, E> {
        ApplyBatch::new(self, self.lists.batch())
    }

    /// Whether an index is being built.
    pub fn rebuilding(&self) -> bool {
        !lock(&self.state).rebuilds.is_empty()
    }

    /// The next level draw of the vector index.
    pub(crate) fn draw(&self) -> f64 {
        lock(&self.draw).next_f64()
    }

    /// The schema an edge of `attr` is applied under: the one a request creates, `created`, else the one being
    /// applied with the previous schema's indexes beside it while its indexes are built, else the state's.
    pub(crate) fn index_schema(
        &self,
        attr: &str,
        created: &BTreeMap<String, SchemaUpdate>,
    ) -> Result<IndexSchema, ApplyError> {
        if let Some(su) = created.get(attr) {
            return IndexSchema::from_update(attr, su);
        }
        let rebuilding = lock(&self.state).rebuilds.get(attr).cloned();
        match rebuilding {
            Some(r) => IndexSchema::during_rebuild(attr, &r.old, &r.new),
            None => IndexSchema::from_state(&self.schema, attr),
        }
    }

    /// Fences `prefixes` at `ts`.
    pub(crate) fn fence(&self, prefixes: Vec<Vec<u8>>, ts: u64) {
        let oldest = self.lists.oldest_open_read();
        lock(&self.state).fence(Fence { ts, prefixes }, oldest);
    }

    /// Records the rebuild of `attr` from `old` to `new`: its job's number.
    pub(crate) fn register_rebuild(&self, attr: &str, old: SchemaUpdate, new: SchemaUpdate) -> u64 {
        let mut state = lock(&self.state);
        state.next_job += 1;
        let id = state.next_job;
        state
            .rebuilds
            .insert(attr.to_string(), Rebuilding { id, old, new });
        id
    }

    /// Ends the rebuild of `attr` by job `id`: whether it was still under way.
    pub(crate) fn end_rebuild(&self, attr: &str, id: u64) -> bool {
        let mut state = lock(&self.state);
        if state.rebuilds.get(attr).is_some_and(|r| r.id == id) {
            state.rebuilds.remove(attr);
            return true;
        }
        false
    }

    /// Whether job `id` still rebuilds `attr`.
    pub(crate) fn still_rebuilding(&self, attr: &str, id: u64) -> bool {
        lock(&self.state)
            .rebuilds
            .get(attr)
            .is_some_and(|r| r.id == id)
    }

    /// Cancels the rebuilds of the predicates `cancel` selects, returning their schemas before the change.
    pub(crate) fn cancel_rebuilds(
        &self,
        cancel: impl Fn(&str) -> bool,
    ) -> Vec<(String, SchemaUpdate)> {
        let mut state = lock(&self.state);
        let attrs: Vec<String> = state
            .rebuilds
            .keys()
            .filter(|a| cancel(a))
            .cloned()
            .collect();
        attrs
            .into_iter()
            .filter_map(|a| state.rebuilds.remove(&a).map(|r| (a, r.old)))
            .collect()
    }
}
