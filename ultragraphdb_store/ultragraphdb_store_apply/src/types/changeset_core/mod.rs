/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod changeset_core_drop;
mod changeset_core_rebuild;
mod changeset_core_record;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use ultragraphdb_core_catalog::Catalog;
use ultragraphdb_core_keys::Region;
use ultragraphdb_core_mapping::Changeset;
use ultragraphdb_store_posting::{CachedList, PostingError, Writes};
use ultragraphdb_store_quads::{QuadStore, QuadWrite, WriteTerms};
use ultragraphdb_traits_store::{ListCache, Store};

use crate::errors::changeset_error::ChangesetError;
use crate::types::applied::Applied;
use crate::types::changeset_write::ChangesetWrite;
use crate::types::executed::Executed;
use crate::types::execution::Execution;
use crate::types::level_draw::LevelDraw;
use crate::types::predicate_prefix::PredicatePrefix;
use crate::types::reads::Reads;
use crate::types::rebuild::Rebuild;
use crate::utils::lock::lock;

/// What a [`crate::ChangesetApplier`] holds, shared with the builds it runs in the background: the quads, the
/// catalogs loaded, the lock commits, drops and swaps take, the build under way, and the vector index's level draws.
#[derive(Debug)]
pub(crate) struct ChangesetCore<S, C> {
    quads: QuadStore<S, C>,
    catalogs: Mutex<BTreeMap<u64, Arc<Catalog>>>,
    apply: Mutex<()>,
    rebuild: Mutex<Option<Rebuild>>,
    draw: Mutex<LevelDraw>,
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
            draw: Mutex::new(LevelDraw::new(seed)),
        }
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
        let cw = ChangesetWrite::new(write, catalog, &self.draw)
            .under(running.as_ref(), running.is_some());
        Ok(Execution::new(namespace, read_ts, cw))
    }

    /// Applies `changeset` on its own, as [`crate::ChangesetApplier::apply`] does: executed and written as a round of
    /// one at the next version, under the lock rounds take, so that nothing commits between the two.
    pub fn apply(&self, changeset: &Changeset) -> Result<(Applied, Option<u64>), ChangesetError> {
        let _apply = lock(&self.apply);
        let executed = self.execute(changeset)?;
        let version = self.quads.lists().next_version();
        let (mut applied, started) = self.write_round(vec![(version, executed)])?;
        let applied = applied
            .pop()
            .expect("a round of one execution gives one commit");
        Ok((applied, started))
    }

    /// Writes `round`, as [`crate::ChangesetApplier::apply_round`] does, with the number of the record of the build
    /// one of its executions started.
    pub fn apply_round(&self, round: Vec<(u64, Executed)>) -> Result<RoundOutcome, ChangesetError> {
        let _apply = lock(&self.apply);
        self.write_round(round)
    }

    /// Writes `round` under the apply lock the caller holds. A record naming the indexes its declarations build and
    /// remove is written and synced before the round's batch; once the batch is written, the catalogs are installed,
    /// the indexes removed are dropped and the build is registered, and a record no build keeps is removed.
    fn write_round(&self, mut round: Vec<(u64, Executed)>) -> Result<RoundOutcome, ChangesetError> {
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
        let Some(&(first, _)) = round.first() else {
            return Ok((Vec::new(), None));
        };
        let record = self.record_round(first, &mut round)?;
        if let Some(r) = &running {
            for (_, executed) in &mut round {
                self.touch(r, std::mem::take(&mut executed.touched));
            }
        }
        let writes: Vec<(u64, &Writes)> = round.iter().map(|(v, e)| (*v, &e.writes)).collect();
        self.quads.lists().commit_round(&writes)?;
        let mut removals: Vec<PredicatePrefix> = Vec::new();
        let mut started = None;
        let mut applied = Vec::with_capacity(round.len());
        for (version, executed) in round {
            let written: BTreeSet<Region> = executed
                .writes
                .written_keys()
                .chain(executed.writes.entry_keys())
                .filter_map(|k| Region::of(k).ok())
                .collect();
            lock(&self.catalogs).insert(executed.catalog.namespace(), Arc::new(executed.catalog));
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
