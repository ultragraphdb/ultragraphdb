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

/// Applies changesets to the quads of one store: each changeset one request, checked against its namespace's catalog,
/// its statements written to the quad lists with their index, count and statistics entries, committed whole or not at
/// all. Commits, drops and the swaps of index builds are serialised by one lock, as the single writer of this stage
/// serialises them. A namespace's catalog is loaded on first use; a namespace with none is given its initial catalog by
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

    /// Applies `changeset` on its own: its commit, or its refusal, which writes nothing.
    pub fn apply(&self, changeset: &Changeset) -> Result<Applied, ChangesetError> {
        self.apply_batch(std::slice::from_ref(changeset))?.remove(0)
    }

    /// Applies `changesets` in one store batch, each a request reading the writes of those before it and committed
    /// at its own timestamp: each one's commit or refusal. A refused changeset writes nothing, and the ones after it
    /// do not see it. A build a declaration of the batch starts is spawned once the batch commits.
    ///
    /// # Errors
    ///
    /// The store's error when the batch cannot be written.
    pub fn apply_batch(
        &self,
        changesets: &[Changeset],
    ) -> Result<Vec<Result<Applied, ChangesetError>>, ChangesetError> {
        let (results, started) = self.core.apply_batch(changesets)?;
        if let Some(record) = started {
            let core = Arc::clone(&self.core);
            self.executor.spawn(move || core.run_rebuild(record));
        }
        Ok(results)
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
