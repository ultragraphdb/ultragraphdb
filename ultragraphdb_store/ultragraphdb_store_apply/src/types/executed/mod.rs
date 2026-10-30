/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use ultragraphdb_core_catalog::Catalog;
use ultragraphdb_core_keys::Region;
use ultragraphdb_store_posting::Writes;
use ultragraphdb_store_quads::WriteTerms;

use crate::types::build_end::BuildEnd;
use crate::types::drop_target::DropTarget;
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
    pub(crate) defaults: Vec<Vec<u8>>,
    pub(crate) default_uids: Vec<u64>,
    pub(crate) catalog_changed: bool,
    pub(crate) pending: Option<Rebuild>,
    pub(crate) touched: BTreeSet<(u64, u64)>,
    pub(crate) removals: Vec<PredicatePrefix>,
    /// What a drop decided in a round drops, and the key ranges it writes, each from its start to the key past its last
    /// (7.7).
    pub(crate) dropped: Option<DropTarget>,
    pub(crate) drop_ranges: Vec<(Vec<u8>, Vec<u8>)>,
    /// The end of a build sequenced through the commit step this execution is: it writes nothing itself, and its
    /// round writes it at its version.
    pub(crate) build_end: Option<BuildEnd>,
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
        defaults: (Vec<Vec<u8>>, Vec<u64>, bool),
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
            defaults: defaults.0,
            default_uids: defaults.1,
            catalog_changed: defaults.2,
            pending,
            touched,
            removals,
            dropped: None,
            drop_ranges: Vec::new(),
            build_end: None,
        }
    }

    /// A drop of `dropped` in `namespace` read at `read_ts`, writing `ranges`.
    pub(crate) fn dropping(
        namespace: u64,
        read_ts: u64,
        catalog: Catalog,
        dropped: Option<DropTarget>,
        ranges: Vec<(Vec<u8>, Vec<u8>)>,
    ) -> Self {
        Self {
            dropped,
            drop_ranges: ranges,
            ..Self::new(
                namespace,
                read_ts,
                Writes::default(),
                WriteTerms::new(namespace),
                Reads::default(),
                BTreeSet::new(),
                0,
                catalog,
                (Vec::new(), Vec::new(), false),
                None,
                BTreeSet::new(),
                Vec::new(),
            )
        }
    }

    /// The end of a build of `namespace` read at `read_ts`, as its proposal to the commit step.
    pub(crate) fn ending(namespace: u64, read_ts: u64, catalog: Catalog, end: BuildEnd) -> Self {
        Self {
            build_end: Some(end),
            ..Self::dropping(namespace, read_ts, catalog, None, Vec::new())
        }
    }

    /// The end of a build this execution is, `None` for any other: a round of its own (decided 2026-10-28).
    pub fn build_end(&self) -> Option<BuildEnd> {
        self.build_end
    }

    /// What a drop decided in a round drops, `None` for any other execution and for a drop of a predicate or a graph
    /// the store does not know.
    pub fn dropped(&self) -> Option<DropTarget> {
        self.dropped
    }

    /// Whether the execution is a drop decided in a round: its round closes after it (7.7).
    pub fn is_drop(&self) -> bool {
        self.dropped.is_some() || !self.drop_ranges.is_empty()
    }

    /// The key ranges a drop writes, each from its start to the key past its last; none for any other execution.
    pub fn drop_ranges(&self) -> &[(Vec<u8>, Vec<u8>)] {
        &self.drop_ranges
    }

    /// Whether the execution's declaration starts an index build: its round holds no other (7.7).
    pub fn starts_build(&self) -> bool {
        self.pending.is_some()
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

    /// The catalog keys it writes as a predicate's implicit default entry, the first write of an undeclared
    /// predicate, sorted: an insert-if-absent on the key, not a catalog change (decided 2026-10-27).
    pub fn defaults(&self) -> &[Vec<u8>] {
        &self.defaults
    }
}
