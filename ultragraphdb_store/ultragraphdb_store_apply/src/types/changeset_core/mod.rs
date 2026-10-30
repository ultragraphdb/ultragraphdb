/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod changeset_core_drop;
mod changeset_core_operate;
mod changeset_core_rebuild;
mod changeset_core_record;
mod changeset_core_replay;
mod changeset_core_vectors;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use ultragraphdb_core_catalog::Catalog;
use ultragraphdb_core_keys::{QuadKey, Region};
use ultragraphdb_core_mapping::Changeset;
use ultragraphdb_store_posting::{CachedList, PostingError, Writes};
use ultragraphdb_store_quads::{QuadStore, QuadWrite, WriteTerms};
use ultragraphdb_traits_store::{ListCache, Store, Temporary, VersionlessEntry};

use crate::errors::changeset_error::ChangesetError;
use crate::types::applied::Applied;
use crate::types::build_end::BuildEnd;
use crate::types::changeset_write::ChangesetWrite;
use crate::types::drop_target::DropTarget;
use crate::types::executed::Executed;
use crate::types::execution::Execution;
use crate::types::predicate_prefix::PredicatePrefix;
use crate::types::reads::Reads;
use crate::types::rebuild::Rebuild;
use crate::types::vector_queue::VectorQueue;
use crate::utils::lock::lock;

/// The lists a build made, by graph, then by key: a vector index's, built in memory.
pub(crate) type BuiltVectors = BTreeMap<u64, BTreeMap<Vec<u8>, Vec<u8>>>;

/// What a sequenced build made, kept until its swap's round: its temporary keyspace and its vector lists.
#[derive(Debug)]
pub(crate) struct Built {
    pub temporary: Option<Temporary>,
    pub vectors: BuiltVectors,
}

/// What a [`crate::ChangesetApplier`] holds, shared with the builds it runs in the background: the quads, the
/// catalogs loaded, the lock commits, drops and swaps take, the build under way, the seed of the vector index's level
/// draws, and each vector predicate's queue, which every applied round feeds and the vector indexer drains.
/// Sequenced (decided 2026-10-28), a build ends with a proposal for the commit step instead of swapping itself in: the
/// ends waiting to be taken, and what each build made, until its swap's round.
#[derive(Debug)]
pub(crate) struct ChangesetCore<S, C> {
    quads: QuadStore<S, C>,
    catalogs: Mutex<BTreeMap<u64, Arc<Catalog>>>,
    apply: Mutex<()>,
    rebuild: Mutex<Option<Rebuild>>,
    seed: u64,
    vectors: Mutex<BTreeMap<(u64, u64), VectorQueue>>,
    sequenced: AtomicBool,
    ended: Mutex<Vec<BuildEnd>>,
    built: Mutex<BTreeMap<u64, Built>>,
}

/// What a round's application gives: each execution's commit, and the record of the build it started.
type RoundOutcome = (Vec<Applied>, Option<u64>);

impl<S, C> ChangesetCore<S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    pub fn new(quads: QuadStore<S, C>, seed: u64) -> Self {
        Self {
            quads,
            catalogs: Mutex::new(BTreeMap::new()),
            apply: Mutex::new(()),
            rebuild: Mutex::new(None),
            seed,
            vectors: Mutex::new(BTreeMap::new()),
            sequenced: AtomicBool::new(false),
            ended: Mutex::new(Vec::new()),
            built: Mutex::new(BTreeMap::new()),
        }
    }

    /// Sequences builds through the commit step from now on, as [`crate::ChangesetApplier::sequence_builds`] does.
    pub fn sequence_builds(&self) {
        self.sequenced.store(true, Ordering::Release);
    }

    pub(crate) fn sequenced(&self) -> bool {
        self.sequenced.load(Ordering::Acquire)
    }

    /// The oldest end of a sequenced build not yet taken.
    pub fn take_build_end(&self) -> Option<BuildEnd> {
        let mut ended = lock(&self.ended);
        (!ended.is_empty()).then(|| ended.remove(0))
    }

    pub(crate) fn push_build_end(&self, end: BuildEnd) {
        lock(&self.ended).push(end);
    }

    pub(crate) fn keep_built(&self, record: u64, built: Built) {
        lock(&self.built).insert(record, built);
    }

    pub(crate) fn take_built(&self, record: u64) -> Option<Built> {
        lock(&self.built).remove(&record)
    }

    /// The proposal of `end`, as [`crate::ChangesetApplier::prepare_build_end`] makes it: at the watermark, in the
    /// build's namespace while it is held, else namespace 0, writing the catalog entry of the declaration the end
    /// writes, none when the build is no longer held.
    pub fn prepare_build_end(&self, end: BuildEnd) -> Result<Executed, ChangesetError> {
        let record = match end {
            BuildEnd::Swap(r) | BuildEnd::Undo(r) => r,
        };
        let held = lock(&self.rebuild).clone().filter(|r| r.record == record);
        let namespace = held.as_ref().map_or(0, |r| r.namespace);
        let lists = self.quads.lists();
        let read_ts = lists.applied_ts();
        let catalog = (*self.catalog(namespace)?).clone();
        // The record names the declaration the end writes, so a replay can restore the build from it (7.9).
        let mut request = lists.request(read_ts);
        if let Some(r) = &held {
            let written = match end {
                BuildEnd::Swap(_) => r.new.clone(),
                BuildEnd::Undo(_) => r.old.clone(),
            };
            request.add_entry(Catalog::entry(
                r.namespace,
                r.predicate,
                &ultragraphdb_core_mapping::Declaration::Predicate(written),
                0,
            ));
        }
        let mut ending = Executed::ending(namespace, read_ts, catalog, end);
        ending.writes = request.into_writes();
        Ok(ending)
    }

    pub fn quads(&self) -> &QuadStore<S, C> {
        &self.quads
    }

    /// The catalog of `namespace` as the last commit left it.
    pub fn catalog(&self, namespace: u64) -> Result<Arc<Catalog>, ChangesetError> {
        if let Some(catalog) = lock(&self.catalogs).get(&namespace) {
            return Ok(catalog.clone());
        }
        let snapshot = self
            .quads
            .store()
            .snapshot(self.quads.lists().applied_ts())?;
        let catalog = Arc::new(Catalog::load(&snapshot, namespace)?);
        lock(&self.catalogs).insert(namespace, catalog.clone());
        Ok(catalog)
    }

    pub fn forget_catalogs(&self) {
        lock(&self.catalogs).clear();
    }

    /// Whether an index is being built.
    pub fn rebuilding(&self) -> bool {
        lock(&self.rebuild).is_some()
    }

    /// Executes `changeset` at the watermark, as [`crate::ChangesetApplier::execute`] does: under no lock the commits
    /// take, so executions run beside each other and beside a round being written.
    pub fn execute(&self, changeset: &Changeset) -> Result<Executed, ChangesetError> {
        let read_ts = self.quads.lists().applied_ts();
        let mut execution = self.begin_execution(changeset.namespace(), read_ts)?;
        execution.apply(changeset)?;
        Ok(execution.finish())
    }

    /// An execution in `namespace` reading at `read_ts`, as [`crate::ChangesetApplier::begin`] begins it: the build
    /// running now and the catalog the last applied round left.
    pub fn begin_execution(
        &self,
        namespace: u64,
        read_ts: u64,
    ) -> Result<Execution<'_, S, C>, ChangesetError> {
        let running = lock(&self.rebuild).clone();
        let catalog = (*self.catalog(namespace)?).clone();
        let write = QuadWrite::new(&self.quads, namespace, self.quads.lists().request(read_ts));
        let cw = ChangesetWrite::new(write, catalog).under(running.as_ref(), running.is_some());
        Ok(Execution::new(namespace, read_ts, cw))
    }

    /// Applies `changeset` on its own, as [`crate::ChangesetApplier::apply`] does: executed and written as a round of
    /// one at the next version, under the lock rounds take, so that nothing commits between the two.
    pub fn apply(&self, changeset: &Changeset) -> Result<(Applied, Option<u64>), ChangesetError> {
        let _apply = lock(&self.apply);
        let executed = self.execute(changeset)?;
        let version = self.quads.lists().next_version();
        let (mut applied, started) = self.write_round(vec![(version, executed)], &[], 0)?;
        let applied = applied
            .pop()
            .expect("a round of one execution gives one commit");
        Ok((applied, started))
    }

    /// Writes `round` with `system` and `through`, as [`crate::ChangesetApplier::apply_round_with`] does, with the
    /// number of the record of the build one of its executions started.
    pub fn apply_round_with(
        &self,
        round: Vec<(u64, Executed)>,
        system: &[VersionlessEntry],
        through: u64,
    ) -> Result<RoundOutcome, ChangesetError> {
        let _apply = lock(&self.apply);
        self.write_round(round, system, through)
    }

    /// Writes `round` under the apply lock the caller holds. A record naming the indexes its declarations build and
    /// remove is written and synced before the round's batch; once the batch is written, each execution's vectors are
    /// queued for the indexer in commit order, the catalogs are installed, the indexes removed are dropped and the
    /// build is registered, and a record no build keeps is removed.
    fn write_round(
        &self,
        mut round: Vec<(u64, Executed)>,
        system: &[VersionlessEntry],
        through: u64,
    ) -> Result<RoundOutcome, ChangesetError> {
        let running = lock(&self.rebuild).clone();
        let mut building = running.is_some();
        for (_, executed) in &round {
            if let Some(pending) = &executed.pending {
                if building {
                    return Err(ChangesetError::Rebuilding(pending.new.predicate().as_str()));
                }
                building = true;
            }
        }
        let mut after = self.quads.lists().applied_ts();
        for (version, _) in &round {
            if *version <= after {
                return Err(PostingError::StaleVersion(*version, after).into());
            }
            after = *version;
        }
        // The watermark ends at the round's last version, which a round ending in a build's end that wrote nothing,
        // an undo of a build a drop cancelled, reaches only so.
        let through = through.max(round.last().map_or(0, |(version, _)| *version));
        let Some(&(first, _)) = round.first() else {
            self.quads.lists().commit_round_with(&[], system, through)?;
            return Ok((Vec::new(), None));
        };
        let record = self.record_round(first, &mut round)?;
        if let Some(r) = &running {
            for (_, executed) in &mut round {
                self.touch(r, std::mem::take(&mut executed.touched));
            }
        }
        // A drop and a build's end write nothing in the batch: each writes at its version once the batch is written,
        // and the system's entries and `through` follow, so the watermark stays below that version until then.
        let writes: Vec<(u64, &Writes)> = round
            .iter()
            .filter(|(_, e)| e.dropped.is_none() && e.build_end.is_none())
            .map(|(v, e)| (*v, &e.writes))
            .collect();
        let drops: Vec<(u64, u64, DropTarget)> = round
            .iter()
            .filter_map(|(v, e)| e.dropped.map(|target| (*v, e.namespace, target)))
            .collect();
        // A build's end writes at its version once the batch is written, as a drop does.
        let ends: Vec<(u64, BuildEnd)> = round
            .iter()
            .filter_map(|(v, e)| e.build_end.map(|end| (*v, end)))
            .collect();
        let deferred = !drops.is_empty() || !ends.is_empty();
        if deferred {
            self.quads.lists().commit_round_with(&writes, &[], 0)?;
        } else {
            self.quads
                .lists()
                .commit_round_with(&writes, system, through)?;
        }
        let mut removals: Vec<PredicatePrefix> = Vec::new();
        let mut started = None;
        let mut applied = Vec::with_capacity(round.len());
        for (version, executed) in round {
            self.queue_vectors(version, &executed);
            let written: BTreeSet<Region> = executed
                .writes
                .written_keys()
                .chain(executed.writes.entry_keys())
                .filter_map(|k| Region::of(k).ok())
                .collect();
            self.install(
                executed.catalog,
                &executed.default_uids,
                executed.catalog_changed,
            )?;
            removals.extend(executed.removals);
            if let Some(mut rebuild) = executed.pending {
                rebuild.start_ts = version;
                started = Some(rebuild);
            }
            applied.push(Applied::new(
                version,
                written,
                executed.read,
                executed.lists_read,
                executed.reads,
            ));
        }
        self.drop_removed(&removals)?;
        for &(version, namespace, target) in &drops {
            self.drop_at(namespace, target, version)?;
        }
        for &(version, end) in &ends {
            self.end_build(end, version)?;
        }
        if deferred {
            self.quads.lists().commit_round_with(&[], system, through)?;
        }
        let started = match (started, record) {
            (Some(rebuild), _) => Some(self.begin(rebuild)),
            (None, Some(record)) => {
                self.remove_record(record)?;
                None
            }
            (None, None) => None,
        };
        Ok((applied, started))
    }
}

impl<S, C> ChangesetCore<S, C> {
    /// Queues for the indexer each vector `executed`, committed at `version`, wrote or removed: every forward list it
    /// wrote of a predicate its catalog declares with a vector index, a transaction's, an operation's or a replayed
    /// record's alike.
    fn queue_vectors(&self, version: u64, executed: &Executed) {
        let changed = executed.writes.written_keys().filter_map(|key| {
            let Ok((
                QuadKey::Forward {
                    namespace,
                    graph,
                    predicate,
                    subject,
                },
                None,
            )) = QuadKey::parse(key)
            else {
                return None;
            };
            executed
                .catalog
                .predicate(predicate)
                .and_then(|d| d.vector_index())
                .map(|_| ((namespace, predicate), (version, graph, subject)))
        });
        let mut queues = None;
        for (queue, change) in changed {
            queues
                .get_or_insert_with(|| lock(&self.vectors))
                .entry(queue)
                .or_insert_with(VectorQueue::default)
                .pending
                .push(change);
        }
    }

    /// Installs the catalog an execution leaves. One that changed the catalog is installed with every predicate the
    /// installed catalog holds and it does not: the implicit default entries other executions wrote since it read,
    /// which are no catalog change and so did not abort it. One that only wrote default entries adds each to the
    /// installed catalog; one that did neither leaves the catalog alone. The catalog installed is the one the store
    /// holds after the round.
    fn install(
        &self,
        mut catalog: Catalog,
        defaults: &[u64],
        changed: bool,
    ) -> Result<(), ChangesetError> {
        let mut catalogs = lock(&self.catalogs);
        let namespace = catalog.namespace();
        let installed = catalogs.get(&namespace);
        let merged = match installed {
            None => catalog,
            Some(installed) if changed => {
                for (uid, declaration) in installed.predicates() {
                    if catalog.predicate(uid).is_none() {
                        catalog.declare_predicate(uid, declaration.clone())?;
                    }
                }
                catalog
            }
            Some(_) if defaults.is_empty() => return Ok(()),
            Some(installed) => {
                let mut merged = (**installed).clone();
                // Written as the store's batch writes them, the later write of a key last. A default entry over a
                // declaration of its round is never admitted together with it: the declaration is a catalog change.
                for &uid in defaults {
                    if let Some(default) = catalog.predicate(uid) {
                        merged.declare_predicate(uid, default.clone())?;
                    }
                }
                merged
            }
        };
        catalogs.insert(namespace, Arc::new(merged));
        Ok(())
    }
}

/// The read set of the write of namespace `namespace` that `request` and `terms` hold.
pub(crate) fn reads_of<S, C>(
    namespace: u64,
    request: &ultragraphdb_store_posting::Request<'_, S, C>,
    terms: &WriteTerms,
) -> Reads
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    Reads::new(
        namespace,
        request.read_keys().map(<[u8]>::to_vec).collect(),
        request
            .tested()
            .map(|(key, object)| (key.to_vec(), object.to_vec()))
            .collect(),
        request
            .ranges()
            .map(|(start, end)| (start.to_vec(), end.map(<[u8]>::to_vec)))
            .collect(),
        terms.absent().map(<[u8]>::to_vec).collect(),
    )
}
