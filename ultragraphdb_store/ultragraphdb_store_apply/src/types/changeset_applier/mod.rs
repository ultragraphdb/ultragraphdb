/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;

use oxrdf::{GraphNameRef, NamedNodeRef};
use ultragraphdb_core_catalog::Catalog;
use ultragraphdb_core_mapping::Changeset;
use ultragraphdb_store_posting::{Batch, CachedList, Lists};
use ultragraphdb_store_quads::QuadStore;
use ultragraphdb_traits_index::VectorIndexOptions;
use ultragraphdb_traits_store::{Executor, ListCache, Store, VersionlessEntry};

use crate::errors::changeset_error::ChangesetError;
use crate::types::applied::Applied;
use crate::types::build_end::BuildEnd;
use crate::types::changeset_core::ChangesetCore;
use crate::types::drop_request::DropRequest;
use crate::types::drop_target::DropTarget;
use crate::types::executed::Executed;
use crate::types::execution::Execution;
use crate::types::operation::Operation;
use crate::types::record_writes::RecordWrites;

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

    /// Sequences every later build through the commit step (decided 2026-10-28): a build ends with a proposal
    /// ([`ChangesetApplier::take_build_end`]) instead of swapping itself in at a version of its own, so every write
    /// takes its version from the step. What the pipeline asks of the applier it drives; an applier driven directly,
    /// by tests and tools, swaps its builds in itself.
    pub fn sequence_builds(&self) {
        self.core.sequence_builds();
    }

    /// The oldest end of a sequenced build not yet taken: a build done, to swap in, or failed or cancelled by a drop,
    /// to undo. Each build ends once.
    pub fn take_build_end(&self) -> Option<BuildEnd> {
        self.core.take_build_end()
    }

    /// The execution writing a logged record's writes, `record`, in `namespace`, for recovery to apply at the record's
    /// commit version (7.9): at the watermark, reading nothing, each entry with the metadata its keyspace gives it, so
    /// the round writes what the record's first apply wrote.
    ///
    /// # Errors
    ///
    /// The store's error, and a list delta that is no posting list.
    pub fn prepare_replay(
        &self,
        namespace: u64,
        record: RecordWrites<'_>,
    ) -> Result<Executed, ChangesetError> {
        self.core.prepare_replay(namespace, record)
    }

    /// The drop of `target` in `namespace`, a logged record's operation, for recovery to apply at the record's commit
    /// version (7.9).
    ///
    /// # Errors
    ///
    /// The store's error loading the namespace's catalog.
    pub fn prepare_replayed_drop(
        &self,
        namespace: u64,
        target: DropTarget,
    ) -> Result<Executed, ChangesetError> {
        self.core.prepare_replayed_drop(namespace, target)
    }

    /// Restores the build `end` ends, for recovery (7.9): from the catalog entry `key` and `value` the end's record
    /// names, the declaration it writes, and the predicate's declaration the store holds, its interim one, so the
    /// round of `end`'s proposal ([`ChangesetApplier::prepare_build_end`]) builds it again and swaps it in, or undoes
    /// it. Not restored, `false`, when the store no longer declares the predicate: a drop cancelled the build.
    ///
    /// # Errors
    ///
    /// The store's error, and an entry that is no declaration.
    pub fn restore_build(
        &self,
        namespace: u64,
        end: BuildEnd,
        key: &[u8],
        value: &[u8],
    ) -> Result<bool, ChangesetError> {
        self.core.restore_build(namespace, end, key, value)
    }

    /// The proposal of `end` for the commit step: an execution at the watermark that writes nothing itself, whose
    /// round writes the swap or the undo at the version the step gives it, alone in its round.
    ///
    /// # Errors
    ///
    /// The store's error loading the build's namespace's catalog.
    pub fn prepare_build_end(&self, end: BuildEnd) -> Result<Executed, ChangesetError> {
        self.core.prepare_build_end(end)
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
        self.apply_round_with(round, &[], 0)
    }

    /// Applies `round` as [`ChangesetApplier::apply_round`] does, with `system`, the commit step's own entries
    /// without versions (its session table, 7.8), written in the round's batch, after its drop when it ends in one,
    /// and `through`, the version of the round's last decision, recorded as applied when it is above the round's last
    /// execution: a round ending in, or holding only, session registrations moves the watermark past them (decided
    /// 2026-10-28).
    ///
    /// # Errors
    ///
    /// As [`ChangesetApplier::apply_round`], and the posting lists' `StaleVersion` for a `through` below the
    /// watermark.
    pub fn apply_round_with(
        &self,
        round: Vec<(u64, Executed)>,
        system: &[VersionlessEntry],
        through: u64,
    ) -> Result<Vec<Applied>, ChangesetError> {
        let (applied, started) = self.core.apply_round_with(round, system, through)?;
        self.spawn(started);
        Ok(applied)
    }

    /// Runs `operations`, the commit-time operations of the transaction `executed` describes, in order, against the
    /// store at the watermark with `overlay`'s layers above it (the writes of every decided but unapplied round) and
    /// `executed`'s own writes on top (7.5), and `dictionary`, the dictionary entries the unapplied writes made, read as
    /// the transaction's own (decided 2026-10-28). Gives the transaction as it commits, its writes and the operations' in one
    /// execution, and appends each operation's result to `results`: the UID of the node an insert-if-absent or
    /// upsert-and-link found or created and whether it created it, zero and `false` for a compare-and-set and a counter
    /// add. `executed` is left as it was; the operations' reads are no part of its read set.
    ///
    /// # Errors
    ///
    /// [`ChangesetError::OperationFailed`] for a compare-and-set whose expected value is not held, or a counter that
    /// is no integer or would overflow; [`ChangesetError::NotUnique`] for an insert-if-absent on a predicate not
    /// declared unique; a change the catalog refuses; the store's error.
    pub fn operate(
        &self,
        executed: &Executed,
        operations: &[Operation],
        overlay: &Batch<'_, S, C>,
        dictionary: &[&VersionlessEntry],
        results: &mut Vec<(u64, bool)>,
    ) -> Result<Executed, ChangesetError> {
        self.core
            .operate(executed, operations, overlay, dictionary, results)
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

    /// One pass of the vector indexer of `predicate` in `namespace`: every vector the rounds applied since its last pass
    /// wrote or removed, in commit order, inserted into the predicate's HNSW lists of its graph, or its node marked
    /// dead, each node's vector read at the watermark, and the lists written at the next version, nothing committing
    /// between the two. Transactions write vectors in their forward lists alone; the indexer is what writes the HNSW
    /// lists. The version indexed through: the watermark the pass read at.
    ///
    /// # Errors
    ///
    /// [`ChangesetError::NoVectorIndex`] when the catalog declares `predicate` with no vector index; the store's error.
    pub fn index_vectors(
        &self,
        namespace: u64,
        predicate: NamedNodeRef<'_>,
    ) -> Result<u64, ChangesetError> {
        self.core.index_vectors(namespace, predicate)
    }

    /// The version the vector indexer of `predicate` in `namespace` has indexed through: every vector a round applied
    /// at or below it is in the HNSW lists; 0 before its first pass.
    ///
    /// # Errors
    ///
    /// The store's error reading the dictionary.
    pub fn indexed_version(
        &self,
        namespace: u64,
        predicate: NamedNodeRef<'_>,
    ) -> Result<u64, ChangesetError> {
        self.core.indexed_version(namespace, predicate)
    }

    /// The nodes of at most `k` vectors of `predicate` in `graph` of `namespace` nearest `query`, best first, by the
    /// predicate's vector index as `options` ask, read at `read_ts`. A search reads the HNSW lists, and returns only
    /// nodes whose vector is visible at its snapshot and which the lists hold: a vector committed and not yet indexed
    /// is absent until the indexer reaches it. An index holding no vector finds none.
    ///
    /// # Errors
    ///
    /// [`ChangesetError::NoVectorIndex`] when the catalog declares `predicate` with no vector index; the store's error;
    /// a stored list that does not decode.
    #[allow(clippy::too_many_arguments)]
    pub fn search_vectors(
        &self,
        namespace: u64,
        graph: GraphNameRef<'_>,
        predicate: NamedNodeRef<'_>,
        query: &[f32],
        k: usize,
        options: &VectorIndexOptions<f32>,
        read_ts: u64,
    ) -> Result<Vec<u64>, ChangesetError> {
        self.core
            .search_vectors(namespace, graph, predicate, query, k, options, read_ts)
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

    /// The drop `request` asks in `namespace` as a proposal of the commit step (7.7): an execution writing the key
    /// ranges of what it drops and nothing else, which [`ChangesetApplier::apply_round`] drops at the version the step
    /// gives it, once its round's batch is written; a round closes after a drop. A predicate or a graph the store does
    /// not know, or a blank-node graph, is dropped as nothing.
    ///
    /// # Errors
    ///
    /// The store's error reading the catalog or the predicate's graphs.
    pub fn prepare_drop(
        &self,
        namespace: u64,
        request: DropRequest<'_>,
    ) -> Result<Executed, ChangesetError> {
        self.core.prepare_drop(namespace, request)
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
