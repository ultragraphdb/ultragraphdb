/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The derivation of a logged transaction's keys from its logical record, at the watermark: the derivation its first
//! apply ran on its net effect ([`crate::types::changeset_write::ChangesetWrite::derive`]).

use ultragraphdb_store_posting::{CachedList, Writes};
use ultragraphdb_store_quads::{QuadWrite, WriteTerms};
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{ListCache, Store, VersionlessEntry};

use crate::errors::changeset_error::ChangesetError;
use crate::types::changeset_core::ChangesetCore;
use crate::types::changeset_write::ChangesetWrite;
use crate::types::executed::Executed;
use crate::types::logical_writes::LogicalWrites;
use crate::utils::lock::lock;
use crate::utils::mark_keys::mark_entry;
use crate::utils::referrer_candidates::referrer_candidates;

impl<S, C, B> ChangesetCore<S, C, B>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    B: BlockSource,
{
    /// The execution deriving `writes` in `tenant`, reading `dictionary` as its own, as
    /// [`crate::ChangesetApplier::prepare_logical`] makes it.
    ///
    /// # Errors
    ///
    /// The errors [`crate::ChangesetApplier::prepare_logical`] names.
    pub fn prepare_logical(
        &self,
        tenant: u64,
        writes: &LogicalWrites,
        dictionary: &[&VersionlessEntry],
    ) -> Result<Executed, ChangesetError> {
        let read_ts = self.quads.lists().applied_ts();
        let running = lock(&self.rebuild).clone();
        let catalog = (*self.catalog(tenant)?).clone();
        let mut write = QuadWrite::new(&self.quads, tenant, self.quads.lists().request(read_ts));
        let mut terms = WriteTerms::new(tenant);
        terms.inherit(dictionary);
        write.seed(&Writes::default(), terms)?;
        let mut cw = ChangesetWrite::new(write, catalog).under(running.as_ref(), running.is_some());
        cw.derive(writes)?;
        let outcome = cw.outcome()?;
        let mut request = outcome.request;
        for (key, value) in outcome.terms.entries() {
            request.add_versionless(key, value);
        }
        for marked in writes.marks() {
            request.add_entry(mark_entry(tenant, marked));
        }
        let derived = request.into_writes();
        let referrers =
            referrer_candidates(&self.quads, tenant, read_ts, &derived, &outcome.terms)?;
        Ok(Executed {
            writes: derived,
            marks: writes.marks().to_vec(),
            referrers,
            terms: outcome.terms,
            defaults: outcome.defaults,
            default_ids: outcome.default_ids,
            catalog_changed: outcome.catalog_changed,
            pending: outcome.pending,
            touched: outcome.touched,
            removals: outcome.removals,
            net: writes.clone(),
            ..Executed::empty(tenant, read_ts, outcome.catalog)
        })
    }
}
