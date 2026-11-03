/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Commit-time operations (7.5): a transaction's operations run in the commit step, after its other checks, on top of
//! its own writes, against the store at the watermark with the decided but unapplied rounds' writes stacked above it.

use std::collections::{BTreeMap, BTreeSet};

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_store_posting::{Batch, CachedList};
use ultragraphdb_store_quads::QuadWrite;
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{ListCache, Store, VersionlessEntry};

use crate::errors::changeset_error::ChangesetError;
use crate::types::changeset_core::ChangesetCore;
use crate::types::changeset_write::ChangesetWrite;
use crate::types::executed::Executed;
use crate::types::operation::Operation;
use crate::types::operation_outcome::OperationOutcome;
use crate::utils::lock::lock;

impl<S, C, B> ChangesetCore<S, C, B>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    B: BlockSource,
{
    /// Runs `operations` on top of `executed`, as [`crate::ChangesetApplier::operate`] does. The operations resolve
    /// their terms on a clone of `executed`'s, so a refusal releases what they took beyond `executed`'s, once, and
    /// leaves `executed`'s own holds with it.
    pub fn operate(
        &self,
        executed: &Executed,
        operations: &[Operation],
        overlay: &Batch<'_, S, C>,
        dictionary: &[&VersionlessEntry],
        results: &mut Vec<OperationOutcome>,
    ) -> Result<Executed, ChangesetError> {
        let read_ts = self.quads().lists().applied_ts();
        let mut write = QuadWrite::new(self.quads(), executed.namespace, overlay.request(read_ts));
        let mut terms = executed.terms.clone();
        terms.inherit(dictionary);
        write.seed(&executed.writes, terms)?;
        let running = lock(&self.rebuild).clone();
        let mut cw = ChangesetWrite::new(write, executed.catalog.clone())
            .under(running.as_ref(), running.is_some());
        if let Err(refused) = run(&mut cw, operations, results) {
            cw.release(Some(&executed.terms));
            return Err(refused);
        }
        let outcome = cw.finish(Some(&executed.terms))?;
        let mut request = outcome.request;
        // The transaction's own dictionary entries are among the seeded writes; the operations' new ones are added.
        let seeded: BTreeSet<Vec<u8>> = executed
            .terms
            .entries()
            .into_iter()
            .map(|(key, _)| key)
            .collect();
        for (key, value) in outcome.terms.entries() {
            if !seeded.contains(&key) {
                request.add_versionless(key, value);
            }
        }
        let defaults: BTreeMap<Vec<u8>, SystemId> = executed
            .defaults
            .iter()
            .cloned()
            .zip(executed.default_ids.iter().copied())
            .chain(outcome.defaults.into_iter().zip(outcome.default_ids))
            .collect();
        let (default_keys, default_ids) = defaults.into_iter().unzip();
        let mut touched = executed.touched.clone();
        touched.extend(outcome.touched);
        Ok(Executed {
            writes: request.into_writes(),
            terms: outcome.terms,
            reads: executed.reads.clone(),
            read: executed.read.clone(),
            lists_read: executed.lists_read,
            defaults: default_keys,
            default_ids,
            catalog_changed: executed.catalog_changed || outcome.catalog_changed,
            pending: executed.pending.clone(),
            touched,
            removals: executed.removals.clone(),
            ..Executed::empty(executed.namespace, executed.read_ts, outcome.catalog)
        })
    }
}

/// Runs `operations` on `cw` in order, appending each one's outcome to `results`; the first refusal ends it.
fn run<S, C, B>(
    cw: &mut ChangesetWrite<'_, S, C, B>,
    operations: &[Operation],
    results: &mut Vec<OperationOutcome>,
) -> Result<(), ChangesetError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    B: BlockSource,
{
    cw.initial()?;
    for (index, operation) in operations.iter().enumerate() {
        results.push(cw.operate(operation, index)?);
    }
    Ok(())
}
