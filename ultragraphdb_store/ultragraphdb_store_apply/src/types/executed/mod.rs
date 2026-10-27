/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use ultragraphdb_core_catalog::Catalog;
use ultragraphdb_core_keys::Region;
use ultragraphdb_store_posting::Writes;
use ultragraphdb_store_quads::WriteTerms;

use crate::types::predicate_prefix::PredicatePrefix;
use crate::types::reads::Reads;
use crate::types::rebuild::Rebuild;

/// A changeset executed against the snapshot at the watermark, on a worker, and not yet written: its writes, which the
/// commit step writes at the version it gives it ([`crate::ChangesetApplier::apply_round`]), the terms it resolved, new
/// ones with provisional UIDs, its read set, the catalog its commit installs, the build its declaration starts, the
/// nodes whose vector it changed while a vector index is built, and the index keys its declarations remove.
#[derive(Debug)]
pub struct Executed {
    pub(crate) namespace: u64,
    pub(crate) read_ts: u64,
    pub(crate) writes: Writes,
    pub(crate) terms: WriteTerms,
    pub(crate) reads: Reads,
    pub(crate) read: BTreeSet<Region>,
    pub(crate) lists_read: usize,
    pub(crate) catalog: Catalog,
    pub(crate) pending: Option<Rebuild>,
    pub(crate) touched: BTreeSet<(u64, u64)>,
    pub(crate) removals: Vec<PredicatePrefix>,
}

impl Executed {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        namespace: u64,
        read_ts: u64,
        writes: Writes,
        terms: WriteTerms,
        reads: Reads,
        read: BTreeSet<Region>,
        lists_read: usize,
        catalog: Catalog,
        pending: Option<Rebuild>,
        touched: BTreeSet<(u64, u64)>,
        removals: Vec<PredicatePrefix>,
    ) -> Self {
        Self {
            namespace,
            read_ts,
            writes,
            terms,
            reads,
            read,
            lists_read,
            catalog,
            pending,
            touched,
            removals,
        }
    }

    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    /// The watermark the changeset was executed at, its read version.
    pub fn read_ts(&self) -> u64 {
        self.read_ts
    }

    /// What its commit writes: the deltas, entries and dictionary entries, at the version the commit step gives it.
    pub fn writes(&self) -> &Writes {
        &self.writes
    }

    /// The terms it resolved: the IRIs it created, with their provisional UIDs, and those it found.
    pub fn terms(&self) -> &WriteTerms {
        &self.terms
    }

    /// Its read set.
    pub fn reads(&self) -> &Reads {
        &self.reads
    }
}
