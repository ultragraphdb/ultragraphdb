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
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_store_quads::{QuadErrorEnum, QuadStore, QuadWrite, WriteTerms};
use ultragraphdb_traits_store::{ListCache, Store};

use crate::errors::changeset_error::{ChangesetError, ChangesetErrorEnum};
use crate::types::applied::Applied;
use crate::types::changeset_write::ChangesetWrite;
use crate::types::level_draw::LevelDraw;
use crate::types::predicate_prefix::PredicatePrefix;
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

/// What a batch's application gives: each changeset's commit or refusal, and the record of the build it started.
type BatchOutcome = (Vec<Result<Applied, ChangesetError>>, Option<u64>);

/// A changeset of a batch whose write finished: its index, terms, the catalog it leaves, the regions it read, and the
/// build it starts.
struct Pushed {
    index: usize,
    terms: WriteTerms,
    catalog: Catalog,
    read: BTreeSet<Region>,
    lists_read: usize,
    pending: Option<Rebuild>,
    removals: Vec<PredicatePrefix>,
}

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
            .snapshot(self.quads.lists().latest_ts())?;
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

    /// Applies `changesets` in one store batch, as [`crate::ChangesetApplier::apply_batch`] does, with the number of
    /// the record of the build a declaration of the batch started. A record naming the indexes the batch's
    /// declarations build and remove is written and synced before the batch; once it commits, the indexes removed are
    /// dropped and the build is registered, and a record no build keeps is removed.
    pub fn apply_batch(&self, changesets: &[Changeset]) -> Result<BatchOutcome, ChangesetError> {
        let _apply = lock(&self.apply);
        let running = lock(&self.rebuild).clone();
        let read_ts = self.quads.lists().latest_ts();
        let mut batch = self.quads.lists().batch();
        let mut results: Vec<Option<Result<Applied, ChangesetError>>> = Vec::new();
        let mut pushed: Vec<Pushed> = Vec::new();
        let mut working: BTreeMap<u64, Catalog> = BTreeMap::new();
        let mut building = running.is_some();
        for (index, changeset) in changesets.iter().enumerate() {
            let ns = changeset.namespace();
            let catalog = match working.get(&ns) {
                Some(c) => c.clone(),
                None => (*self.catalog(ns)?).clone(),
            };
            let write = QuadWrite::new(&self.quads, ns, batch.request(read_ts));
            let mut cw =
                ChangesetWrite::new(write, catalog, &self.draw).under(running.as_ref(), building);
            match cw.apply(changeset) {
                Ok(()) => {
                    let outcome = cw.finish();
                    let read: BTreeSet<Region> = outcome
                        .request
                        .read_keys()
                        .filter_map(|k| Region::of(k).ok())
                        .collect();
                    let lists_read = outcome.request.read_keys().count();
                    building |= outcome.pending.is_some();
                    if let Some(r) = &running {
                        self.touch(r, outcome.touched);
                    }
                    working.insert(ns, outcome.catalog.clone());
                    batch.push(outcome.request);
                    pushed.push(Pushed {
                        index,
                        terms: outcome.terms,
                        catalog: outcome.catalog,
                        read,
                        lists_read,
                        pending: outcome.pending,
                        removals: outcome.removals,
                    });
                    results.push(None);
                }
                Err(e) => results.push(Some(Err(e))),
            }
        }
        let record = self.record_batch(&mut pushed)?;
        let terms: Vec<WriteTerms> = pushed.iter().map(|p| p.terms.clone()).collect();
        let written: Vec<BTreeSet<Region>> = written_regions(&batch);
        let committed = self.quads.commit(batch, &terms)?;
        let mut aborted = false;
        let mut removals: Vec<PredicatePrefix> = Vec::new();
        let mut started = None;
        for ((p, result), written) in pushed.into_iter().zip(committed).zip(written) {
            results[p.index] = Some(match result {
                Ok(ts) => {
                    lock(&self.catalogs).insert(p.catalog.namespace(), Arc::new(p.catalog));
                    removals.extend(p.removals);
                    if let Some(mut rebuild) = p.pending {
                        rebuild.start_ts = ts;
                        started = Some(rebuild);
                    }
                    Ok(Applied::new(ts, written, p.read, p.lists_read))
                }
                Err(e) => {
                    aborted |= matches!(e.0, QuadErrorEnum::Aborted);
                    Err(ChangesetError(ChangesetErrorEnum::Quad(e)))
                }
            });
        }
        if aborted {
            // A later write of the batch may have read the catalog a refused one changed.
            self.forget_catalogs();
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
        Ok((
            results
                .into_iter()
                .map(|r| {
                    r.expect(
                        "every changeset was refused before the batch or decided by its commit",
                    )
                })
                .collect(),
            started,
        ))
    }
}

/// The regions of every key each request of `batch` writes, its entries included.
fn written_regions<S, C>(
    batch: &ultragraphdb_store_posting::Batch<'_, S, C>,
) -> Vec<BTreeSet<Region>>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    batch
        .requests()
        .iter()
        .map(|r| {
            r.written_keys()
                .chain(r.entry_keys())
                .filter_map(|k| Region::of(k).ok())
                .collect()
        })
        .collect()
}
