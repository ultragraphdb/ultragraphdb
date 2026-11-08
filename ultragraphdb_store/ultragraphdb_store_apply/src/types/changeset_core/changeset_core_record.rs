/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The rebuild record: written and synced before a round whose declarations build or remove indexes, naming those
//! indexes, in every graph, as prefixes without their graph; removed once the removed indexes are dropped and no build
//! keeps it, or by the batch that swaps a build in or undoes it. A partition that loaded its catalogs finishes each
//! record a crash left: it drops, in every graph, each of its indexes the loaded catalog does not declare, and builds
//! again those it declares, so a crash leaves every catalog's indexes, complete, and nothing else.

use std::collections::BTreeSet;

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_mapping::PredicateDeclaration;

use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{ListCache, RebuildRecord, Store};

use crate::errors::changeset_error::ChangesetError;
use crate::types::changeset_core::ChangesetCore;
use crate::types::executed::Executed;
use crate::types::predicate_prefix::PredicatePrefix;
use crate::types::rebuild::Rebuild;
use crate::utils::quad_prefixes::declared_prefixes;

impl<S, C, B> ChangesetCore<S, C, B>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    B: BlockSource,
{
    /// Writes and syncs the record of the indexes `round`'s declarations build and remove, when they name any, under
    /// the number `id`, the round's first version, which no other round takes: its number, which a build started
    /// keeps.
    pub(super) fn record_round(
        &self,
        id: u64,
        round: &mut [(u64, Executed)],
    ) -> Result<Option<u64>, ChangesetError> {
        let mut prefixes: Vec<PredicatePrefix> = Vec::new();
        for (_, executed) in round.iter() {
            if let Some(rebuild) = &executed.pending {
                prefixes.extend(rebuild.maintained_prefixes());
            }
            prefixes.extend(executed.removals.iter().cloned());
        }
        if prefixes.is_empty() {
            return Ok(None);
        }
        let record =
            RebuildRecord::new(id, prefixes.iter().map(PredicatePrefix::to_bytes).collect());
        self.quads.store().write_rebuild_record(&record)?;
        for (_, executed) in round.iter_mut() {
            if let Some(rebuild) = executed.pending.as_mut() {
                rebuild.record = id;
            }
        }
        Ok(Some(id))
    }

    /// Removes record `id` in a batch of its own, taking no version: the commit step's next version stays free
    /// (decided 2026-10-28).
    pub(super) fn remove_record(&self, id: u64) -> Result<(), ChangesetError> {
        self.quads.lists().remove_rebuild_record(id)?;
        Ok(())
    }

    /// Drops every key under `prefixes`, in every graph holding one, physically, at the watermark: the keys of indexes
    /// no catalog names, which take no version (decided 2026-10-28).
    pub(super) fn drop_removed(&self, prefixes: &[PredicatePrefix]) -> Result<(), ChangesetError> {
        if prefixes.is_empty() {
            return Ok(());
        }
        let lists = self.quads.lists();
        let latest = lists.applied_ts();
        lists.gate(latest)?;
        let snapshot = lists.store().snapshot(latest)?;
        let mut expanded = Vec::new();
        for p in prefixes {
            expanded.extend(p.expand(&snapshot)?);
        }
        if !expanded.is_empty() {
            lists.drop_prefixes_at_watermark(&expanded)?;
        }
        Ok(())
    }

    /// Finishes every record the store keeps, as a crash or a power loss left it, against the catalogs loaded from the
    /// metadata group's versions ([`ChangesetCore::load_catalog`]): drops each of its indexes the loaded catalog does
    /// not declare; and, since an index it names that the catalog declares may be incomplete, runs the build of that
    /// predicate's declared indexes again from the data at the watermark, the catalog holding the interim declaration
    /// until it ends, under the record's number. A record whose indexes the catalog declares none of is removed. What a
    /// partition does once it has loaded its catalogs, before it serves: the number of the record of the build started
    /// again, at most one, since one build runs at a time; a later record is finished by dropping alone.
    ///
    /// # Errors
    ///
    /// The store's error.
    pub fn finish_records(&self) -> Result<Option<u64>, ChangesetError> {
        let records = self.quads.store().rebuild_records()?;
        let mut started = None;
        for record in records {
            let mut undeclared: BTreeSet<PredicatePrefix> = BTreeSet::new();
            let mut rebuilt = None;
            for bytes in record.prefixes() {
                let Some(prefix) = PredicatePrefix::from_bytes(bytes) else {
                    continue;
                };
                let (tenant, predicate) = (prefix.tenant(), prefix.predicate());
                let catalog = self.catalog(tenant)?;
                let declared = catalog
                    .predicate(predicate)
                    .map(|d| declared_prefixes(tenant, predicate, d))
                    .unwrap_or_default();
                if declared.contains(&prefix) {
                    rebuilt = rebuilt.or(Some((tenant, predicate)));
                } else {
                    let _inserted: bool = undeclared.insert(prefix);
                }
            }
            tracing::warn!(
                record = record.id(),
                dropped = undeclared.len(),
                "finishing a rebuild record a crash or power loss left"
            );
            self.drop_removed(&undeclared.into_iter().collect::<Vec<_>>())?;
            match rebuilt.filter(|_| started.is_none()) {
                Some((tenant, predicate)) => {
                    started = self.build_again(tenant, predicate, record.id())?;
                }
                None => self.remove_record(record.id())?,
            }
        }
        Ok(started)
    }

    /// Runs the build of predicate `predicate`'s declared indexes in `tenant` again, under record `record`, from the
    /// data at the watermark: the catalog holds the declaration without its indexes until the build swaps them in.
    fn build_again(
        &self,
        tenant: u64,
        predicate: SystemId,
        record: u64,
    ) -> Result<Option<u64>, ChangesetError> {
        let Some(declared) = self.catalog(tenant)?.predicate(predicate).cloned() else {
            self.remove_record(record)?;
            return Ok(None);
        };
        let mut bare = PredicateDeclaration::new(declared.predicate().clone())
            .with_kind(declared.kind())
            .with_cardinality(declared.cardinality())
            .with_unique(declared.unique())
            .with_domains(declared.domains().to_vec());
        if let Some(datatype) = declared.datatype() {
            bare = bare.with_datatype(datatype.clone());
        }
        let mut rebuild = Rebuild::pending(tenant, predicate, bare, declared);
        rebuild.record = record;
        rebuild.start_ts = self.quads.lists().applied_ts();
        let interim = rebuild.interim();
        self.set_catalog(tenant, |catalog| {
            catalog.declare_predicate(predicate, interim)?;
            Ok(())
        })?;
        debug_assert!(!self.rebuilding());
        Ok(Some(self.begin(rebuild)))
    }
}
