/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod changeset_core_catalog;
mod changeset_core_drop;
mod changeset_core_logical;
mod changeset_core_operate;
mod changeset_core_rebuild;
mod changeset_core_record;
mod changeset_core_referrers;
mod changeset_core_replay;
mod changeset_core_vectors;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use ultragraphdb_core_catalog::{Catalog, encode_declaration};
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{Graph, QuadKey, Region};
use ultragraphdb_core_mapping::{Changeset, Declaration, PredicateDeclaration};
use ultragraphdb_error_invariant::InvariantError;
use ultragraphdb_store_posting::{CachedList, PostingError, Writes};
use ultragraphdb_store_quads::{QuadStore, QuadWrite, WriteTerms};
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_catalog::DefaultEntry;
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
pub(crate) type BuiltVectors = BTreeMap<Graph, BTreeMap<Vec<u8>, Vec<u8>>>;

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
pub(crate) struct ChangesetCore<S, C, B> {
    quads: QuadStore<S, C, B>,
    // Locked state: executions, rounds and the builds on the executor reach the core at once; the flag only ever
    // turns on.
    catalogs: Mutex<BTreeMap<u64, Arc<Catalog>>>,
    apply: Mutex<()>,
    rebuild: Mutex<Option<Rebuild>>,
    seed: u64,
    vectors: Mutex<BTreeMap<(u64, SystemId), VectorQueue>>,
    sequenced: AtomicBool,
    ended: Mutex<Vec<BuildEnd>>,
    built: Mutex<BTreeMap<u64, Built>>,
    /// The default-entry records the rounds applied since the host last took them ask the metadata group for.
    defaults: Mutex<Vec<DefaultEntry>>,
}

/// What a round's application gives: each execution's commit, and the record of the build it started.
type RoundOutcome = (Vec<Applied>, Option<u64>);

/// What settling a round's executions gives: the prefixes of the indexes it removes, the build it starts, and what
/// each applied.
type Settled = (Vec<PredicatePrefix>, Option<Rebuild>, Vec<Applied>);

impl<S, C, B> ChangesetCore<S, C, B>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    B: BlockSource,
{
    pub fn new(quads: QuadStore<S, C, B>, seed: u64) -> Self {
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
            defaults: Mutex::new(Vec::new()),
        }
    }

    /// The default-entry records to send, as [`crate::ChangesetApplier::take_default_entries`] takes them.
    pub fn take_default_entries(&self) -> Vec<DefaultEntry> {
        std::mem::take(&mut *lock(&self.defaults))
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
        let _previous: Option<Built> = lock(&self.built).insert(record, built);
    }

    pub(crate) fn take_built(&self, record: u64) -> Option<Built> {
        lock(&self.built).remove(&record)
    }

    /// The proposal of `end`, as [`crate::ChangesetApplier::prepare_build_end`] makes it: at the watermark, in the
    /// build's tenant while it is held, else tenant 0, under the catalog that tenant holds, whose version
    /// started the build.
    pub fn prepare_build_end(&self, end: BuildEnd) -> Result<Executed, ChangesetError> {
        let record = match end {
            BuildEnd::Swap(r) | BuildEnd::Undo(r) => r,
        };
        let held = lock(&self.rebuild).clone().filter(|r| r.record == record);
        let tenant = held.as_ref().map_or(0, |r| r.tenant);
        let read_ts = self.quads.lists().applied_ts();
        let catalog = (*self.catalog(tenant)?).clone();
        // The end writes nothing of its own: its round swaps or undoes the build at its version, and replay derives
        // the build again from the catalog version its start record names.
        Ok(Executed::ending(tenant, read_ts, catalog, end))
    }

    pub fn quads(&self) -> &QuadStore<S, C, B> {
        &self.quads
    }

    /// The catalog of `tenant` as the last commit left it, held in memory: loaded from the metadata group's version
    /// at start ([`ChangesetCore::load_catalog`]), changed by the rounds since; an empty catalog for a tenant none
    /// was installed for, which its first write gives its initial catalog.
    pub fn catalog(&self, tenant: u64) -> Result<Arc<Catalog>, ChangesetError> {
        let mut catalogs = lock(&self.catalogs);
        let catalog = catalogs
            .entry(tenant)
            .or_insert_with(|| Arc::new(Catalog::new(tenant)));
        debug_assert_eq!(catalog.tenant(), tenant);
        Ok(catalog.clone())
    }

    pub fn forget_catalogs(&self) {
        lock(&self.catalogs).clear();
    }

    /// Changes the catalog of `tenant` held in memory with `change`, as a drop or a build's end changes it outside
    /// an execution's commit.
    pub(crate) fn set_catalog(
        &self,
        tenant: u64,
        change: impl FnOnce(&mut Catalog) -> Result<(), ChangesetError>,
    ) -> Result<(), ChangesetError> {
        let mut catalog = (*self.catalog(tenant)?).clone();
        change(&mut catalog)?;
        let _previous: Option<Arc<Catalog>> =
            lock(&self.catalogs).insert(tenant, Arc::new(catalog));
        Ok(())
    }

    /// Whether an index is being built.
    pub fn rebuilding(&self) -> bool {
        lock(&self.rebuild).is_some()
    }

    /// The record of the build running, as [`crate::ChangesetApplier::build_record`] gives it.
    pub fn build_record(&self) -> Option<u64> {
        lock(&self.rebuild).as_ref().map(|r| r.record)
    }

    /// Executes `changeset` at the watermark, as [`crate::ChangesetApplier::execute`] does: under no lock the commits
    /// take, so executions run beside each other and beside a round being written.
    pub fn execute(&self, changeset: &Changeset) -> Result<Executed, ChangesetError> {
        let read_ts = self.quads.lists().applied_ts();
        let mut execution = self.begin_execution(changeset.tenant(), read_ts)?;
        match execution.apply(changeset) {
            Ok(()) => execution.finish(),
            Err(refused) => {
                // A refused changeset commits nothing: its terms are released, writing nothing, so it holds no pending
                // ID and no entry of its terms reaches the store outside a commit.
                self.quads.release(execution.terms());
                Err(refused)
            }
        }
    }

    /// An execution in `tenant` reading at `read_ts`, as [`crate::ChangesetApplier::begin`] begins it: the build
    /// running now and the catalog the last applied round left.
    pub fn begin_execution(
        &self,
        tenant: u64,
        read_ts: u64,
    ) -> Result<Execution<'_, S, C, B>, ChangesetError> {
        let running = lock(&self.rebuild).clone();
        let catalog = (*self.catalog(tenant)?).clone();
        let write = QuadWrite::new(&self.quads, tenant, self.quads.lists().request(read_ts));
        let cw = ChangesetWrite::new(write, catalog).under(running.as_ref(), running.is_some());
        Ok(Execution::new(tenant, read_ts, cw))
    }

    /// Applies `changeset` on its own, as [`crate::ChangesetApplier::apply`] does: executed and written as a round of
    /// one at the next version, under the lock rounds take, so that nothing commits between the two.
    pub fn apply(&self, changeset: &Changeset) -> Result<(Applied, Option<u64>), ChangesetError> {
        let _apply = lock(&self.apply);
        let executed = self.execute(changeset)?;
        let version = self.quads.lists().next_version();
        let (mut applied, started) = self.write_round(vec![(version, executed)], &[], 0)?;
        let applied = applied.pop().ok_or(InvariantError::violated(
            "a round of one execution gives one commit",
        ))?;
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
        self.check_round(running.is_some(), &round)?;
        // The watermark ends at the round's last version, which a round ending in a build's end that wrote nothing,
        // an undo of a build a drop cancelled, reaches only so.
        let through = through.max(round.last().map_or(0, |(version, _)| *version));
        let Some(&(first, _)) = round.first() else {
            self.quads.lists().commit_round_with(&[], system, through)?;
            return Ok((Vec::new(), None));
        };
        let referrers = self.derive_referrers(&mut round)?;
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
            .filter_map(|(v, e)| e.dropped.map(|target| (*v, e.tenant, target)))
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
        // The batch is written: the sets it holds are the stored ones, and a drop below forgets its tenant's.
        for ((tenant, entity), set) in referrers {
            self.quads.hold_referrers(tenant, entity, set);
        }
        let (removals, started, applied) = self.settle_round(round)?;
        self.drop_removed(&removals)?;
        for &(version, tenant, target) in &drops {
            let _dropped_at: u64 = self.drop_at(tenant, target, version)?;
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

    /// Refuses `round` when it starts a build while one is `building` or another of its executions starts one, or
    /// when its versions do not rise above the watermark.
    fn check_round(&self, building: bool, round: &[(u64, Executed)]) -> Result<(), ChangesetError> {
        let mut building = building;
        for (_, executed) in round {
            if let Some(pending) = &executed.pending {
                if building {
                    return Err(ChangesetError::rebuilding(pending.new.predicate().as_str()));
                }
                building = true;
            }
        }
        let mut after = self.quads.lists().applied_ts();
        for (version, _) in round {
            if *version <= after {
                return Err(PostingError::stale_version(*version, after).into());
            }
            after = *version;
        }
        Ok(())
    }

    /// Settles each execution of `round` once its batch is written: its vectors queued, its catalog installed, and
    /// what it applied; with the prefixes of the indexes the round removes and the build it starts.
    fn settle_round(&self, round: Vec<(u64, Executed)>) -> Result<Settled, ChangesetError> {
        let mut removals: Vec<PredicatePrefix> = Vec::new();
        let mut started = None;
        let mut applied = Vec::with_capacity(round.len());
        for (version, executed) in round {
            // The batch holding its dictionary entries, its pending terms' included, is written: its pending IDs are
            // settled.
            self.quads.settle(&executed.terms);
            self.queue_vectors(version, &executed);
            let written: BTreeSet<Region> = executed
                .writes
                .written_keys()
                .chain(executed.writes.entry_keys())
                .filter_map(|k| Region::of(k).ok())
                .collect();
            let catalog = match executed.catalog_version {
                Some(number) => executed.catalog.with_version(number),
                None => executed.catalog,
            };
            self.install(catalog, &executed.default_ids, executed.catalog_changed)?;
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
        Ok((removals, started, applied))
    }
}

impl<S, C, B> ChangesetCore<S, C, B> {
    /// Asks the metadata group for the default entry of each predicate of `defaults` that `catalog`, an execution's,
    /// holds with its default entry and `installed` does not hold: a first write of the predicate since the partition
    /// started, which the group finds recorded when another partition, or this one before a restart, asked first.
    fn ask_defaults(
        &self,
        installed: Option<&Catalog>,
        catalog: &Catalog,
        defaults: &[SystemId],
    ) -> Result<(), ChangesetError> {
        let asked = defaults.iter().filter_map(|uid| {
            let default = catalog.predicate(*uid)?;
            let held = installed.and_then(|c| c.predicate(*uid)).is_some();
            (!held).then(|| -> Result<DefaultEntry, ChangesetError> {
                let entry = encode_declaration(&Declaration::Predicate(default.clone()))?;
                Ok(DefaultEntry::new(
                    catalog.tenant(),
                    default.predicate().as_str().to_string(),
                    entry,
                ))
            })
        });
        let asked = asked.collect::<Result<Vec<DefaultEntry>, ChangesetError>>()?;
        if !asked.is_empty() {
            lock(&self.defaults).extend(asked);
        }
        Ok(())
    }

    /// Queues for the indexer each vector `executed`, committed at `version`, wrote or removed: every forward list it
    /// wrote of a predicate its catalog declares with a vector index, a transaction's, an operation's or a replayed
    /// record's alike.
    fn queue_vectors(&self, version: u64, executed: &Executed) {
        let changed = executed.writes.written_keys().filter_map(|key| {
            let Ok((
                QuadKey::Forward {
                    tenant,
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
                .map(|_| ((tenant, predicate), (version, graph, subject)))
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
        defaults: &[SystemId],
        changed: bool,
    ) -> Result<(), ChangesetError> {
        let mut catalogs = lock(&self.catalogs);
        let tenant = catalog.tenant();
        let installed = catalogs.get(&tenant);
        self.ask_defaults(installed.map(|c| &**c), &catalog, defaults)?;
        let merged = match installed {
            None => catalog,
            Some(installed) if changed => {
                // Only implicit default entries: a declaration the execution's catalog left out was removed by it.
                for (uid, declaration) in installed.predicates() {
                    let implicit =
                        *declaration == PredicateDeclaration::new(declaration.predicate().clone());
                    if implicit && catalog.predicate(uid).is_none() {
                        catalog.declare_predicate(uid, declaration.clone())?;
                    }
                }
                catalog
            }
            Some(_) if defaults.is_empty() => return Ok(()),
            Some(installed) => {
                let mut merged = (**installed).clone();
                // A default entry never replaces a declaration: one installed since the execution read is kept.
                for &uid in defaults {
                    if let Some(default) = catalog.predicate(uid)
                        && merged.predicate(uid).is_none()
                    {
                        merged.declare_predicate(uid, default.clone())?;
                    }
                }
                merged
            }
        };
        let _previous: Option<Arc<Catalog>> = catalogs.insert(tenant, Arc::new(merged));
        Ok(())
    }
}

/// The read set of the write of tenant `tenant` that `request` and `terms` hold, with `points`, the keys besides
/// lists it read whole: the marks it read.
pub(crate) fn reads_of<S, C>(
    tenant: u64,
    request: &ultragraphdb_store_posting::Request<'_, S, C>,
    terms: &WriteTerms,
    points: &[Vec<u8>],
) -> Reads
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    let mut lists: Vec<Vec<u8>> = request.read_keys().map(<[u8]>::to_vec).collect();
    if !points.is_empty() {
        lists.extend_from_slice(points);
        lists.sort_unstable();
        lists.dedup();
    }
    Reads::new(
        tenant,
        lists,
        request
            .tested()
            .map(|(key, object)| (key.to_vec(), object.to_vec()))
            .collect(),
        request
            .ranges()
            .map(|(start, end)| (start.to_vec(), end.map(<[u8]>::to_vec)))
            .collect(),
        terms.absent().collect(),
    )
}
