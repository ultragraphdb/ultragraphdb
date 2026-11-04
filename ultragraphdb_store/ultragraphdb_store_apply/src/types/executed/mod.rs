/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;
use std::num::NonZeroU64;

use ultragraphdb_core_catalog::Catalog;
use ultragraphdb_core_ids::{BucketGroup, SystemId};
use ultragraphdb_core_keys::{Graph, QuadKey, Region};
use ultragraphdb_store_posting::DEL;
use ultragraphdb_store_posting::Writes;
use ultragraphdb_store_quads::{WriteTerms, posting_object};

use crate::types::logical_quad::LogicalQuad;
use crate::types::marked_reference::MarkedReference;

use crate::types::build_end::BuildEnd;
use crate::types::drop_target::DropTarget;
use crate::types::predicate_prefix::PredicatePrefix;
use crate::types::reads::Reads;
use crate::types::rebuild::Rebuild;

/// A changeset executed against the snapshot at the watermark, on a worker, and not yet written: its writes, which the
/// commit step writes at the version it gives it ([`crate::ChangesetApplier::apply_round`]), the terms it resolved, new
/// ones with the IDs they took while it ran, its read set, the catalog its commit installs, the build its declaration starts, the
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
    pub(crate) default_ids: Vec<SystemId>,
    pub(crate) catalog_changed: bool,
    pub(crate) pending: Option<Rebuild>,
    pub(crate) touched: BTreeSet<(Graph, SystemId)>,
    pub(crate) removals: Vec<PredicatePrefix>,
    /// What a drop decided in a round drops, and the key ranges it writes, each from its start to the key past its last
    /// (7.7).
    pub(crate) dropped: Option<DropTarget>,
    pub(crate) drop_ranges: Vec<(Vec<u8>, Vec<u8>)>,
    /// The end of a build sequenced through the commit step this execution is: it writes nothing itself, and its
    /// round writes it at its version.
    pub(crate) build_end: Option<BuildEnd>,
    /// The number of the catalog version this execution installs, a version's taking effect in the partition
    /// ([`crate::ChangesetApplier::execute_catalog`]); `None` for every other execution.
    pub(crate) catalog_version: Option<u64>,
    pub(crate) marks: Vec<MarkedReference>,
    /// The home's own groups the execution's edges reference entities of the store's buckets from, each `(entity,
    /// group)` whose referrer set lacked the group at the read version: its round adds each still missing at
    /// generation 1 with the commit version's epoch (partitions, "The home adds its own group without a
    /// registration").
    pub(crate) referrers: Vec<(SystemId, BucketGroup)>,
    /// The referrer-set member a registration's round sets, `(entity, group, generation)` ([`crate::Registering`]).
    pub(crate) registration: Option<(SystemId, BucketGroup, NonZeroU64)>,
}

impl Executed {
    /// An execution of `namespace` read at `read_ts` under `catalog` that reads and writes nothing.
    pub(crate) fn empty(namespace: u64, read_ts: u64, catalog: Catalog) -> Self {
        Self {
            namespace,
            read_ts,
            writes: Writes::default(),
            terms: WriteTerms::new(namespace),
            reads: Reads::default(),
            read: BTreeSet::new(),
            lists_read: 0,
            catalog,
            defaults: Vec::new(),
            default_ids: Vec::new(),
            catalog_changed: false,
            pending: None,
            touched: BTreeSet::new(),
            removals: Vec::new(),
            dropped: None,
            drop_ranges: Vec::new(),
            build_end: None,
            catalog_version: None,
            marks: Vec::new(),
            referrers: Vec::new(),
            registration: None,
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
            ..Self::empty(namespace, read_ts, catalog)
        }
    }

    /// The end of a build of `namespace` read at `read_ts`, as its proposal to the commit step.
    pub(crate) fn ending(namespace: u64, read_ts: u64, catalog: Catalog, end: BuildEnd) -> Self {
        Self {
            build_end: Some(end),
            ..Self::dropping(namespace, read_ts, catalog, None, Vec::new())
        }
    }

    /// The execution's writes in their logical form (write-ahead-log, "A round's record is logical"): each statement a
    /// forward list's delta sets, once per statement ID its posting carries, each statement one removes, and each
    /// statement record written, by graph and statement ID. The deltas hold the execution's net effect on its snapshot,
    /// derived from it by the derivation [`crate::ChangesetApplier::prepare_logical`] runs, so a replay of these writes
    /// derives every key and value again.
    ///
    /// # Errors
    ///
    /// The store's corruption error for a forward key that does not parse or a posting that names no object.
    pub fn logical(
        &self,
    ) -> Result<crate::types::logical_writes::LogicalWrites, crate::ChangesetError> {
        let (mut inserted, mut removed) = (Vec::new(), Vec::new());
        for (key, delta) in self.writes.deltas() {
            let Ok((
                QuadKey::Forward {
                    graph,
                    predicate,
                    subject,
                    ..
                },
                None,
            )) = QuadKey::parse(key)
            else {
                continue;
            };
            for posting in delta.postings() {
                let object = posting_object(key, posting)?;
                let quad = |statement| {
                    LogicalQuad::new(graph, subject, predicate, object.clone(), statement)
                };
                match (posting.op(), posting.statement_ids()) {
                    (DEL, _) => removed.push(quad(None)),
                    (_, []) => inserted.push(quad(None)),
                    (_, ids) => inserted.extend(ids.iter().map(|id| quad(Some(*id)))),
                }
            }
        }
        let records = self
            .writes
            .entries()
            .iter()
            .filter_map(|e| match QuadKey::parse(e.key()) {
                Ok((
                    QuadKey::Statement {
                        graph, statement, ..
                    },
                    None,
                )) => Some((graph, statement, e.value().map(<[u8]>::to_vec))),
                _ => None,
            })
            .collect();
        Ok(crate::types::logical_writes::LogicalWrites::new(
            inserted, removed, records,
        ))
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

    /// The namespace the changeset was executed in.
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

    /// The terms it resolved: the IRIs it created or took from a concurrent creator, with their IDs, and those it found.
    pub fn terms(&self) -> &WriteTerms {
        &self.terms
    }

    /// Its read set.
    pub fn reads(&self) -> &Reads {
        &self.reads
    }

    /// The number of the catalog version it installs, a version taking effect in the partition, `None` for any other
    /// execution: its record is the build start naming that version.
    pub fn catalog_version(&self) -> Option<u64> {
        self.catalog_version
    }

    /// The catalog version of the build this execution ends, `None` for any other execution: the version its namespace's
    /// catalog holds, the one that started the build, since a later version waits for the build's end.
    pub fn build_version(&self) -> Option<u64> {
        self.build_end.map(|_| self.catalog.version())
    }

    /// The catalog keys it writes as a predicate's implicit default entry, the first write of an undeclared
    /// predicate, sorted: an insert-if-absent on the key, not a catalog change (decided 2026-10-27).
    pub fn defaults(&self) -> &[Vec<u8>] {
        &self.defaults
    }

    /// The registrations the execution relied on as a referrer, in the order marked: its commit writes each mark and
    /// its record lists each (write-set record section 4).
    pub fn marks(&self) -> &[MarkedReference] {
        &self.marks
    }
}
