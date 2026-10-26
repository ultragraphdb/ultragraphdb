/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The rebuild record: written and synced before a batch whose declarations build or remove indexes, naming those
//! indexes, in every graph, as prefixes without their graph; removed once the removed indexes are dropped and no build
//! keeps it, or by the batch that swaps a build in or undoes it. A store opened with a record drops, in every graph,
//! each of its indexes the loaded catalog does not declare, so a crash leaves every catalog's indexes, complete, and
//! nothing else.

use std::collections::BTreeSet;

use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_store::{ListCache, RebuildRecord, Store};

use crate::errors::changeset_error::ChangesetError;
use crate::types::changeset_core::{ChangesetCore, Pushed};
use crate::types::predicate_prefix::PredicatePrefix;
use crate::utils::quad_prefixes::declared_prefixes;

impl<S, C> ChangesetCore<S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    /// Writes and syncs the record of the indexes `pushed`'s declarations build and remove, when they name any: its
    /// number, which a build started keeps.
    pub(super) fn record_batch(
        &self,
        pushed: &mut [Pushed],
    ) -> Result<Option<u64>, ChangesetError> {
        let mut prefixes: Vec<PredicatePrefix> = Vec::new();
        for p in pushed.iter() {
            if let Some(rebuild) = &p.pending {
                prefixes.extend(rebuild.maintained_prefixes());
            }
            prefixes.extend(p.removals.iter().cloned());
        }
        if prefixes.is_empty() {
            return Ok(None);
        }
        let id = self.quads.lists().fresh_ts();
        let record =
            RebuildRecord::new(id, prefixes.iter().map(PredicatePrefix::to_bytes).collect());
        self.quads.store().write_rebuild_record(&record)?;
        for rebuild in pushed.iter_mut().filter_map(|p| p.pending.as_mut()) {
            rebuild.record = id;
        }
        Ok(Some(id))
    }

    /// Removes record `id` in a batch of its own.
    pub(super) fn remove_record(&self, id: u64) -> Result<(), ChangesetError> {
        let lists = self.quads.lists();
        let mut request = lists.request(lists.latest_ts());
        request.remove_rebuild_record_on_commit(id);
        let mut batch = lists.batch();
        batch.push(request);
        lists.commit(batch)?;
        Ok(())
    }

    /// Drops every key under `prefixes`, in every graph holding one, physically.
    pub(super) fn drop_removed(&self, prefixes: &[PredicatePrefix]) -> Result<(), ChangesetError> {
        if prefixes.is_empty() {
            return Ok(());
        }
        let lists = self.quads.lists();
        let latest = lists.latest_ts();
        lists.gate(latest)?;
        let snapshot = lists.store().snapshot(latest)?;
        let mut expanded = Vec::new();
        for p in prefixes {
            expanded.extend(p.expand(&snapshot)?);
        }
        if !expanded.is_empty() {
            lists.drop_prefixes_now(&expanded, Vec::new())?;
        }
        Ok(())
    }

    /// Finishes every record the store keeps, as a crash or a power loss left it: drops each of its indexes the loaded
    /// catalog does not declare, then removes it. What the applier does when it opens, before it serves.
    pub fn finish_records(&self) -> Result<(), ChangesetError> {
        let records = self.quads.store().rebuild_records()?;
        for record in records {
            let mut undeclared: BTreeSet<PredicatePrefix> = BTreeSet::new();
            for bytes in record.prefixes() {
                let Some(prefix) = PredicatePrefix::from_bytes(bytes) else {
                    continue;
                };
                let catalog = self.catalog(prefix.namespace())?;
                let declared = catalog
                    .predicate(prefix.predicate())
                    .map(|d| declared_prefixes(prefix.namespace(), prefix.predicate(), d))
                    .unwrap_or_default();
                if !declared.contains(&prefix) {
                    undeclared.insert(prefix);
                }
            }
            tracing::warn!(
                record = record.id(),
                dropped = undeclared.len(),
                "finishing a rebuild record a crash or power loss left"
            );
            self.drop_removed(&undeclared.into_iter().collect::<Vec<_>>())?;
            self.remove_record(record.id())?;
        }
        Ok(())
    }
}
