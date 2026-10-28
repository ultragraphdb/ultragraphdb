/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;

use oxrdf::{GraphNameRef, NamedNodeRef};
use ultragraphdb_core_catalog::Catalog;
use ultragraphdb_core_mapping::Changeset;
use ultragraphdb_store_posting::{CachedList, Lists};
use ultragraphdb_store_quads::QuadStore;
use ultragraphdb_traits_store::{Executor, ListCache, Store};

use crate::errors::changeset_error::ChangesetError;
use crate::types::applied::Applied;
use crate::types::changeset_core::ChangesetCore;
use crate::types::executed::Executed;
use crate::types::execution::Execution;

/// Applies changesets to the quads of one store: each changeset executed against the snapshot at the watermark in a
/// request of its own, checked against its namespace's catalog, its statements written to the quad lists with their
/// index, count and statistics entries, and committed whole or not at all, in a round at the version the commit step
/// gives it. Executions run beside each other; rounds, drops and the swaps of index builds are serialised by one lock. A namespace's catalog is loaded on first use; a namespace with none is given its initial catalog by
/// its first write.
///
/// A declaration adding an index to a predicate holding data builds it on `executor`, in the background: the catalog
/// holds the interim declaration until one batch swaps the index in, and writes meanwhile maintain the old
/// declaration's indexes and the new one's. One build runs at a time. Drops remove what they drop physically.
#[derive(Debug)]
pub struct ChangesetApplier<S, C, E> {
    core: Arc<ChangesetCore<S, C>>,
    executor: E,
}

impl<S, C, E> ChangesetApplier<S, C, E>
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    E: Executor,
{
    /// The applier of `lists`' store, its builds on `executor`, its vector index's level draws seeded by `seed`.
    /// Refused when the store was written under another mapping contract. A rebuild record a crash left is finished
    /// first.
    pub fn new(lists: Lists<S, C>, executor: E, seed: u64) -> Result<Self, ChangesetError> {
        Self::over(QuadStore::open(lists)?, executor, seed)
    }

    /// The applier of `quads`, as [`ChangesetApplier::new`] makes it.
    pub fn over(quads: QuadStore<S, C>, executor: E, seed: u64) -> Result<Self, ChangesetError> {
        let core = ChangesetCore::new(quads, seed);
        core.finish_records()?;
        Ok(Self {
            core: Arc::new(core),
            executor,
        })
    }

    pub fn quads(&self) -> &QuadStore<S, C> {
        self.core.quads()
    }

    pub fn executor(&self) -> &E {
        &self.executor
    }

    /// The catalog of `namespace` as the last commit left it: what queries read, the interim declaration of a
    /// predicate whose index is being built included.
    pub fn catalog(&self, namespace: u64) -> Result<Arc<Catalog>, ChangesetError> {
        self.core.catalog(namespace)
    }

    /// Forgets every loaded catalog, as a reopening of the store needs.
    pub fn forget_catalogs(&self) {
        self.core.forget_catalogs();
    }

    /// Whether an index is being built.
    pub fn rebuilding(&self) -> bool {
        self.core.rebuilding()
    }

    /// Executes `changeset` against the snapshot at the watermark, as a worker does, writing nothing: its terms
    /// resolved through the dictionary, a term new to it given a provisional UID from the leases, its operations
    /// checked against the catalog the last applied round left, its writes built in a request of its own, and its
    /// reads recorded. Executions do not wait for each other or for the commit step.
    ///
    /// # Errors
    ///
    /// The changeset's refusal, which writes nothing.
    pub fn execute(&self, changeset: &Changeset) -> Result<Executed, ChangesetError> {
        self.core.execute(changeset)
    }

    /// Begins an execution in `namespace` reading at `read_ts`, held open across a transaction's requests: the build
    /// running now and the catalog the last applied round left, as [`ChangesetApplier::execute`] reads them. A
    /// `read_ts` above the watermark makes its reads wait at the read gate until the watermark reaches it, so a caller
    /// refuses one first.
    ///
    /// # Errors
    ///
    /// The store's error loading the namespace's catalog.
    pub fn begin(
        &self,
        namespace: u64,
        read_ts: u64,
    ) -> Result<Execution<'_, S, C>, ChangesetError> {
        self.core.begin_execution(namespace, read_ts)
    }

    /// Writes `round`, the admitted executions of one round with their commit versions, in round order, as one store
    /// batch: the store and the posting cache take every one at its version, then the catalogs they leave are
    /// installed, the indexes their declarations remove are dropped and the build one starts is begun, in round order.
    /// Each one's [`Applied`]. An empty round writes nothing.
    ///
    /// # Errors
    ///
    /// [`ChangesetError::Rebuilding`] when an execution of the round starts a build while one runs or another of the
    /// round starts one, and a version not above the one before it, the watermark for the first, as the posting
    /// lists' `StaleVersion`; nothing is written. The store's error when the batch cannot be written.
    pub fn apply_round(&self, round: Vec<(u64, Executed)>) -> Result<Vec<Applied>, ChangesetError> {
        let (applied, started) = self.core.apply_round(round)?;
        self.spawn(started);
        Ok(applied)
    }

    /// Applies `changeset` on its own: executed at the watermark and written as a round of one at the next version,
    /// nothing committing between the two. Its commit, or its refusal, which writes nothing. A build its declaration
    /// starts is spawned once it commits.
    ///
    /// # Errors
    ///
    /// The changeset's refusal; the store's error when its round cannot be written.
    pub fn apply(&self, changeset: &Changeset) -> Result<Applied, ChangesetError> {
        let (applied, started) = self.core.apply(changeset)?;
        self.spawn(started);
        Ok(applied)
    }

    /// Spawns the build of record `started` on the executor, when one was started.
    fn spawn(&self, started: Option<u64>) {
        if let Some(record) = started {
            let core = Arc::clone(&self.core);
            self.executor.spawn(move || core.run_rebuild(record));
        }
    }

    /// Drops `predicate` of `namespace` in every graph, with its catalog entries, its statement records, and its
    /// entries in the subject- and object-predicate lists: the drop's timestamp.
    pub fn drop_predicate(
        &self,
        namespace: u64,
        predicate: NamedNodeRef<'_>,
    ) -> Result<u64, ChangesetError> {
        self.core.drop_predicate(namespace, predicate)
    }

    /// Drops every key of `graph` in `namespace`: the drop's timestamp.
    pub fn drop_graph(
        &self,
        namespace: u64,
        graph: GraphNameRef<'_>,
    ) -> Result<u64, ChangesetError> {
        self.core.drop_graph(namespace, graph)
    }

    /// Drops the data of `namespace` and keeps its catalog: the drop's timestamp.
    pub fn drop_data(&self, namespace: u64) -> Result<u64, ChangesetError> {
        self.core.drop_data(namespace)
    }

    /// Drops everything in `namespace` and writes its initial catalog at timestamp 1: the drop's timestamp.
    pub fn drop_namespace(&self, namespace: u64) -> Result<u64, ChangesetError> {
        self.core.drop_namespace(namespace)
    }

    /// Drops every quad key and writes the root namespace's initial catalog at timestamp 1; the dictionary is kept:
    /// the drop's timestamp.
    pub fn drop_all(&self) -> Result<u64, ChangesetError> {
        self.core.drop_all()
    }
}
