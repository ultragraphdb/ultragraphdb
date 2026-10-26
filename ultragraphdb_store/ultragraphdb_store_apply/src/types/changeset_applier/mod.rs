/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use ultragraphdb_core_catalog::Catalog;
use ultragraphdb_core_keys::Region;
use ultragraphdb_core_mapping::Changeset;
use ultragraphdb_store_posting::{CachedList, Lists};
use ultragraphdb_store_quads::{QuadErrorEnum, QuadStore, QuadWrite, WriteTerms};
use ultragraphdb_traits_store::{ListCache, Store};

use crate::errors::changeset_error::{ChangesetError, ChangesetErrorEnum};
use crate::types::applied::Applied;
use crate::types::changeset_write::ChangesetWrite;
use crate::utils::lock::lock;

/// Applies changesets to the quads of one store: each changeset one request, checked against its namespace's catalog,
/// its statements written to the quad lists with their index, count and statistics entries, committed whole or not at
/// all. Commits are serialised by one lock, as the single writer of this stage serialises them. A namespace's catalog
/// is loaded on first use; a namespace with none is given its initial catalog by its first write.
#[derive(Debug)]
pub struct ChangesetApplier<S, C> {
    quads: QuadStore<S, C>,
    catalogs: Mutex<BTreeMap<u64, Arc<Catalog>>>,
    apply: Mutex<()>,
}

impl<S, C> ChangesetApplier<S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    /// The applier of `lists`' store, refused when the store was written under another mapping contract.
    pub fn new(lists: Lists<S, C>) -> Result<Self, ChangesetError> {
        Ok(Self::over(QuadStore::open(lists)?))
    }

    /// The applier of `quads`.
    pub fn over(quads: QuadStore<S, C>) -> Self {
        Self {
            quads,
            catalogs: Mutex::new(BTreeMap::new()),
            apply: Mutex::new(()),
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

    /// Forgets every loaded catalog, as a drop of a namespace's catalog or a reopening of the store needs.
    pub fn forget_catalogs(&self) {
        lock(&self.catalogs).clear();
    }

    /// Applies `changeset` on its own: its commit, or its refusal, which writes nothing.
    pub fn apply(&self, changeset: &Changeset) -> Result<Applied, ChangesetError> {
        self.apply_batch(std::slice::from_ref(changeset))?.remove(0)
    }

    /// Applies `changesets` in one store batch, each a request reading the writes of those before it and committed
    /// at its own timestamp: each one's commit or refusal. A refused changeset writes nothing, and the ones after it
    /// do not see it.
    ///
    /// # Errors
    ///
    /// The store's error when the batch cannot be written.
    pub fn apply_batch(
        &self,
        changesets: &[Changeset],
    ) -> Result<Vec<Result<Applied, ChangesetError>>, ChangesetError> {
        let _apply = lock(&self.apply);
        let read_ts = self.quads.lists().latest_ts();
        let mut batch = self.quads.lists().batch();
        let mut results: Vec<Option<Result<Applied, ChangesetError>>> = Vec::new();
        let mut pushed: Vec<(usize, WriteTerms, Catalog, BTreeSet<Region>)> = Vec::new();
        let mut working: BTreeMap<u64, Catalog> = BTreeMap::new();
        for (i, changeset) in changesets.iter().enumerate() {
            let ns = changeset.namespace();
            let catalog = match working.get(&ns) {
                Some(c) => c.clone(),
                None => (*self.catalog(ns)?).clone(),
            };
            let write = QuadWrite::new(&self.quads, ns, batch.request(read_ts));
            let mut cw = ChangesetWrite::new(write, catalog);
            match cw.apply(changeset) {
                Ok(()) => {
                    let (request, terms, catalog) = cw.finish();
                    let read: BTreeSet<Region> = request
                        .read_keys()
                        .filter_map(|k| Region::of(k).ok())
                        .collect();
                    working.insert(ns, catalog.clone());
                    batch.push(request);
                    pushed.push((i, terms, catalog, read));
                    results.push(None);
                }
                Err(e) => results.push(Some(Err(e))),
            }
        }
        let terms: Vec<WriteTerms> = pushed.iter().map(|(_, t, _, _)| t.clone()).collect();
        let written: Vec<BTreeSet<Region>> = written_regions(&batch);
        let committed = self.quads.commit(batch, &terms)?;
        let mut aborted = false;
        for (((i, _, catalog, read), result), written) in
            pushed.into_iter().zip(committed).zip(written)
        {
            results[i] = Some(match result {
                Ok(ts) => {
                    lock(&self.catalogs).insert(catalog.namespace(), Arc::new(catalog));
                    Ok(Applied::new(ts, written, read))
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
        Ok(results
            .into_iter()
            .map(|r| {
                r.expect("every changeset was refused before the batch or decided by its commit")
            })
            .collect())
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
