/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The rebuild record, which Dgraph does not have: written and synced before a build starts, naming the prefixes
//! writes maintain while it builds; removed by the swap batch, the failure batch, or the batch of a rebuild a drop
//! cancelled; and, found at open, finished by dropping its prefixes the loaded schema does not declare, so a crash
//! mid-rebuild leaves no key that writes maintained for the new schema.

use std::collections::BTreeSet;

use ultragraphdb_core_schema::SchemaMode;
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_store::{Executor, ListCache, RebuildRecord, Store};

use crate::errors::apply_error::ApplyError;
use crate::types::applier::Applier;
use crate::types::index_rebuild::IndexRebuild;
use crate::utils::prefixes::{declared_prefixes, maintained_prefixes};

impl<S, C, E> Applier<S, C, E>
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    E: Executor + Send + Sync + 'static,
{
    /// Writes and syncs the record of `rebuild` before its build starts: its number is the rebuild's timestamp, and it
    /// names the prefixes writes maintain while it builds: the new schema's, the previous schema's, and the reverse
    /// counts the two maintained together keep.
    pub(crate) fn record_rebuild(&self, rebuild: &IndexRebuild) -> Result<(), ApplyError> {
        let prefixes = maintained_prefixes(
            rebuild.attr(),
            rebuild.old_schema(),
            rebuild.current_schema(),
        )?;
        let record = RebuildRecord::new(rebuild.start_ts(), prefixes);
        self.lists.store().write_rebuild_record(&record)?;
        Ok(())
    }

    /// Ends a rebuild whose record could not be written: the previous schema back, rollups resumed.
    pub(crate) fn abandon(&self, id: u64, rebuild: &IndexRebuild) {
        if self.end_rebuild(rebuild.attr(), id)
            && let Some(old) = rebuild.old_schema()
        {
            self.schema.set(rebuild.attr(), old.clone());
        }
        self.schema.delete_mut_schema(rebuild.attr());
        self.lists.resume_rollups();
    }

    /// Removes the record of `rebuild` in a batch of its own: what a rebuild a drop cancelled leaves.
    pub(crate) fn remove_record(&self, rebuild: &IndexRebuild) -> Result<(), ApplyError> {
        self.delete_under(&[], Some(rebuild.start_ts())).map(|_| ())
    }

    /// Finishes the rebuilds a crash or a power loss stopped, as the store keeps their records: drops each record's
    /// prefixes the loaded schema does not declare, physically, then removes the record. What [`Applier::new`] does
    /// once the schema is loaded, before it serves.
    pub(crate) fn finish_rebuilds(&self) -> Result<(), ApplyError> {
        let records = self.lists.store().rebuild_records()?;
        if records.is_empty() {
            return Ok(());
        }
        let mut declared: BTreeSet<Vec<u8>> = BTreeSet::new();
        for attr in self.schema.predicates() {
            if let Some(su) = self.schema.get(SchemaMode::Query, &attr) {
                declared.extend(declared_prefixes(&attr, &su)?);
            }
        }
        for record in records {
            let undeclared: Vec<Vec<u8>> = record
                .prefixes()
                .iter()
                .filter(|p| !declared.contains(*p))
                .cloned()
                .collect();
            tracing::warn!(
                rebuild = record.id(),
                prefixes = record.prefixes().len(),
                dropped = undeclared.len(),
                "finishing a rebuild a crash or power loss stopped"
            );
            if !undeclared.is_empty() {
                self.lists.drop_prefixes_now(&undeclared, Vec::new())?;
            }
            self.delete_under(&[], Some(record.id()))?;
        }
        Ok(())
    }
}
