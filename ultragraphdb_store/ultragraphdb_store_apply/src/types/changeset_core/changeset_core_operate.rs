/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Commit-time operations (7.5): a transaction's operations run in the commit step, after its other checks, on top of
//! its own writes, against the store at the watermark with the decided but unapplied rounds' writes stacked above it.

use std::collections::{BTreeMap, BTreeSet};

use ultragraphdb_store_posting::{Batch, CachedList};
use ultragraphdb_store_quads::QuadWrite;
use ultragraphdb_traits_store::{ListCache, Store, VersionlessEntry};

use crate::errors::changeset_error::ChangesetError;
use crate::types::changeset_core::ChangesetCore;
use crate::types::changeset_write::ChangesetWrite;
use crate::types::executed::Executed;
use crate::types::operation::Operation;
use crate::utils::lock::lock;

impl<S, C> ChangesetCore<S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    /// Runs `operations` on top of `executed`, as [`crate::ChangesetApplier::operate`] does.
    pub fn operate(
        &self,
        executed: &Executed,
        operations: &[Operation],
        overlay: &Batch<'_, S, C>,
        dictionary: &[&VersionlessEntry],
        results: &mut Vec<(u64, bool)>,
    ) -> Result<Executed, ChangesetError> {
        let read_ts = self.quads().lists().applied_ts();
        let mut write = QuadWrite::new(self.quads(), executed.namespace, overlay.request(read_ts));
        let mut terms = executed.terms.clone();
        terms.inherit(dictionary);
        write.seed(&executed.writes, terms)?;
        let running = lock(&self.rebuild).clone();
        let mut cw = ChangesetWrite::new(write, executed.catalog.clone())
            .under(running.as_ref(), running.is_some());
        cw.initial()?;
        for (index, operation) in operations.iter().enumerate() {
            results.push(cw.operate(operation, index)?);
        }
        let outcome = cw.finish();
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
        let defaults: BTreeMap<Vec<u8>, u64> = executed
            .defaults
            .iter()
            .cloned()
            .zip(executed.default_uids.iter().copied())
            .chain(outcome.defaults.into_iter().zip(outcome.default_uids))
            .collect();
        let (default_keys, default_uids) = defaults.into_iter().unzip();
        let mut touched = executed.touched.clone();
        touched.extend(outcome.touched);
        Ok(Executed::new(
            executed.namespace,
            executed.read_ts,
            request.into_writes(),
            outcome.terms,
            executed.reads.clone(),
            executed.read.clone(),
            executed.lists_read,
            outcome.catalog,
            (
                default_keys,
                default_uids,
                executed.catalog_changed || outcome.catalog_changed,
            ),
            executed.pending.clone(),
            touched,
            executed.removals.clone(),
        ))
    }
}
