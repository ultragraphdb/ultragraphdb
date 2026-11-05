/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod execution_partition;

use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;

use oxrdf::{GraphNameRef, NamedNode, NamedNodeRef, NamedOrBlankNodeRef, Term};
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{Keyspace, Region};
use ultragraphdb_core_mapping::Changes;
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{ListCache, Store};

use ultragraphdb_store_quads::{
    KernelRow, KernelScan, Page, PageEnd, PageWalk, QuadError, WriteTerms,
};

use crate::errors::changeset_error::ChangesetError;
use crate::types::changeset_core::reads_of;
use crate::types::changeset_write::ChangesetWrite;
use crate::types::executed::Executed;
use crate::types::marked_reference::MarkedReference;
use crate::types::read_counts::ReadCounts;
use crate::utils::mark_keys::mark_entry;
use crate::utils::referrer_candidates::referrer_candidates;

/// A transaction's execution held open across its requests: one request of the posting lists at its read version,
/// which every changeset applied to it writes and every read reads, so a read sees the execution's own writes on top
/// of its snapshot and nothing another execution wrote. [`Execution::finish`] gives the [`Executed`] a commit writes,
/// its keys derived once from its net effect on its snapshot; dropping it writes nothing and closes its read.
///
/// The changesets applied to one execution behave as one changeset holding their changes in order.
pub struct Execution<'a, S, C, B> {
    tenant: u64,
    read_ts: u64,
    write: ChangesetWrite<'a, S, C, B>,
    /// The registrations relied on as a referrer, in the order marked, one per `(entity, group)`.
    marks: Vec<MarkedReference>,
    /// The mark keys read, each once, recorded in the read set.
    marks_read: Vec<Vec<u8>>,
}

impl<S, C, B> std::fmt::Debug for Execution<'_, S, C, B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Execution")
            .field("tenant", &self.tenant)
            .field("read_ts", &self.read_ts)
            .finish_non_exhaustive()
    }
}

impl<'a, S, C, B> Execution<'a, S, C, B>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    B: BlockSource,
{
    pub(crate) fn new(tenant: u64, read_ts: u64, write: ChangesetWrite<'a, S, C, B>) -> Self {
        Self {
            tenant,
            read_ts,
            write,
            marks: Vec::new(),
            marks_read: Vec::new(),
        }
    }

    /// The tenant the execution writes.
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The version the execution reads at.
    pub fn read_ts(&self) -> u64 {
        self.read_ts
    }

    /// The subjects of the statements the execution's changesets named by their reifier (a delete of the statement, a
    /// property set or removed through it) and resolved, in the order resolved: the reifier names no subject before
    /// execution, so a partition checks its routing against these afterwards. A reifier naming no statement adds none.
    pub fn reified_subjects(&self) -> &[SystemId] {
        self.write.reified_subjects()
    }

    /// Applies every change of `changeset`, in order, on top of what the execution wrote before; the first refusal
    /// ends it, and the execution should then be dropped, since the changes before the refusal stay applied to it.
    ///
    /// # Errors
    ///
    /// The changeset's refusal. The changeset's tenant is not checked: its changes are written in the execution's.
    pub fn apply<W: Changes>(&mut self, changes: &W) -> Result<(), ChangesetError> {
        self.write.apply(changes)
    }

    /// The objects of `subject`'s `predicate` in `graph` as the execution sees them, decoded into `out`, which is
    /// cleared first: the subject's list read whole and recorded in the read set. A graph, subject or predicate the
    /// store does not hold is recorded absent and holds nothing.
    ///
    /// # Errors
    ///
    /// The store's error; corruption of a list or a dictionary entry.
    pub fn objects_into(
        &mut self,
        graph: GraphNameRef<'_>,
        subject: NamedOrBlankNodeRef<'_>,
        predicate: NamedNodeRef<'_>,
        out: &mut Vec<Term>,
    ) -> Result<(), ChangesetError> {
        self.write.objects_into(graph, subject, predicate, out)
    }

    /// The read kernel over this execution's writes: the lists `scan` names, at the execution's read timestamp with its own
    /// uncommitted writes over them, in key order, at most its limit of them, appended to `out`, by the walk a plain read takes.
    /// Nothing is held. What it covered is recorded as a range scanned: to its past key when the range ended, found nothing
    /// included, and to just past the last row when stopped at the limit. The key a next scan begins at when stopped.
    ///
    /// # Errors
    ///
    /// The store's error; corruption of a list; a keyspace holding no lists. Nothing is recorded.
    pub fn scan(
        &mut self,
        scan: &KernelScan,
        out: &mut Vec<KernelRow>,
    ) -> Result<Option<Vec<u8>>, ChangesetError> {
        let page = self.page(scan, KernelRow::gather(out))?;
        Ok(page.into_end().resume_from())
    }

    /// A page of the read kernel over this execution's writes: the lists `scan` names, at the execution's read timestamp with its
    /// own uncommitted writes over them, in key order, streamed to `sink` object by object by the walk a plain page takes
    /// ([`PageWalk`]). Nothing is held. What it covered is recorded as a range scanned before it returns: to the past key when
    /// the range ended, found nothing included, to just past the last key the sink took anything of when stopped at the limit
    /// or paused, and nothing when the sink took nothing.
    ///
    /// # Errors
    ///
    /// The store's error; corruption of a list; a keyspace holding no lists. Nothing is recorded.
    pub fn page<F>(&mut self, scan: &KernelScan, sink: F) -> Result<Page, ChangesetError>
    where
        F: FnMut(&[u8], &EncodedTerm, &[SystemId]) -> ControlFlow<()>,
    {
        scan.check_keyspace()?;
        let (start, end) = scan.bounds();
        let mut walk = PageWalk::new(scan, self.read_ts, sink);
        let walked =
            self.write
                .request()
                .span(start, end, scan.walk_bounds(), |key, request| {
                    walk.visit(key, request).map_err(ChangesetError::from)
                })?;
        Ok(walk.end(walked))
    }

    /// A page of stored values over this execution's writes ([`ultragraphdb_store_quads::QuadStore::values_page_over`]): what `scan`
    /// names at the execution's read timestamp, each key once in key order, the statement records it wrote over what the store
    /// holds and none of those it removed, streamed to `sink`. Its span is a range scanned, recorded before it returns as
    /// [`Execution::page`] records one, so a commit writing a statement's record inside it conflicts.
    ///
    /// # Errors
    ///
    /// The store's error; a keyspace holding no stored values. Nothing is recorded.
    pub fn values_page<F>(&mut self, scan: &KernelScan, sink: F) -> Result<Page, ChangesetError>
    where
        F: FnMut(&[u8], &[u8]) -> ControlFlow<()>,
    {
        scan.check_values().map_err(ChangesetError::from)?;
        let (start, end) = scan.bounds();
        let own = if scan.keyspace() == Keyspace::Statements {
            self.write.own_records(&start, end.as_deref())
        } else {
            BTreeMap::new()
        };
        let quads = self.write.quads();
        quads.lists().gate(self.read_ts)?;
        let page = quads.values_page_over(scan, &own, sink)?;
        let request = self.write.request();
        match page.end() {
            PageEnd::Exhausted => request.record_range(start, end),
            PageEnd::Paused { last: None, .. } => {}
            reached @ (PageEnd::Stopped { .. }
            | PageEnd::Paused { .. }
            | PageEnd::Yielded { .. }) => {
                request.record_range(start, reached.clone().resume_from());
            }
        }
        Ok(page)
    }

    /// The ID `iri` has as this execution's reads name it: got or created, the same ID any concurrent writer of the IRI takes,
    /// one created recorded as an absent term; `None`, recording nothing, for an IRI under the skolem base the dictionary does
    /// not hold, which no read can hold.
    ///
    /// # Errors
    ///
    /// Refusals of creating an ID; the store's error.
    pub fn find_iri(&mut self, iri: NamedNodeRef<'_>) -> Result<Option<SystemId>, ChangesetError> {
        self.write.find_iri(iri)
    }

    /// Whether this execution resolved `iri` already, so its reads name it without asking the dictionary or its home.
    pub fn knows_iri(&mut self, iri: &str) -> bool {
        self.write.terms().knows(iri)
    }

    /// The value of each of `keys` in the dictionary as this execution sees it, into `out`, cleared first: the entries its
    /// terms would write over those the store holds, so a term it created is found.
    ///
    /// # Errors
    ///
    /// The store's error.
    pub fn dictionary_into(
        &mut self,
        keys: &[&[u8]],
        out: &mut Vec<Option<Vec<u8>>>,
    ) -> Result<(), ChangesetError> {
        out.clear();
        let own: BTreeMap<Vec<u8>, Vec<u8>> = self.write.terms().entries().into_iter().collect();
        let store = self.write.quads().store();
        for key in keys {
            out.push(match own.get(*key) {
                Some(value) => Some(value.clone()),
                None => store.get_versionless(key).map_err(QuadError::from)?,
            });
        }
        Ok(())
    }

    /// What this execution's reads come to so far. Marks are counted as read whole whether or not the request read them too,
    /// so this may run above what the read set takes, never below.
    pub fn read_counts(&mut self) -> ReadCounts {
        let absent = self.write.terms().absent().count();
        let request = self.write.request();
        let tested: Vec<(usize, usize)> =
            request.tested().map(|(k, o)| (k.len(), o.len())).collect();
        let (mut key_bytes, mut lists) = (request.range_bytes(), self.marks_read.len());
        for key in request.read_keys() {
            key_bytes += key.len();
            lists += 1;
        }
        key_bytes += self.marks_read.iter().map(Vec::len).sum::<usize>();
        key_bytes += tested.iter().map(|(k, o)| k + o).sum::<usize>();
        ReadCounts::new(
            key_bytes,
            lists,
            tested.len(),
            request.range_count(),
            absent,
        )
    }

    /// The key ranges this execution scanned so far, in order, each from its start, inclusive, to its end, exclusive, or to
    /// the last key when it has none.
    pub fn scanned_ranges(&mut self) -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
        self.write
            .request()
            .ranges()
            .map(|(start, end)| (start.to_vec(), end.map(<[u8]>::to_vec)))
            .collect()
    }

    /// How many lists this execution holds: what a scan leaves unchanged.
    pub fn held_lists(&mut self) -> usize {
        self.write.request().held_count()
    }

    /// Removes predicate `uid`'s declaration, named `iri`, with the indexes it declares: it behaves as the default
    /// again, as a catalog version that no longer declares it asks.
    ///
    /// # Errors
    ///
    /// The store's error reading the predicate's data.
    pub(crate) fn undeclare(
        &mut self,
        uid: SystemId,
        iri: &NamedNode,
    ) -> Result<(), ChangesetError> {
        self.write.undeclare(uid, iri)
    }

    /// Removes entity type or relation `uid`'s declaration, of entry kind `kind`, as a catalog version that no longer
    /// names it asks.
    pub(crate) fn undeclare_named(&mut self, kind: u8, uid: SystemId) {
        self.write.undeclare_named(kind, uid);
    }

    /// The terms the execution resolved so far.
    pub(crate) fn terms(&mut self) -> &WriteTerms {
        self.write.terms()
    }

    /// What the execution leaves to commit: its writes, terms and read set, as [`crate::ChangesetApplier::execute`]
    /// gives them. Its keys are derived once from its net effect on its snapshot, by the derivation replay runs on its
    /// logical writes ([`Executed::logical`]), so a change it undid writes no key; the read set holds what its changes
    /// and the derivation read.
    ///
    /// # Errors
    ///
    /// The store's error, or the corruption of a list or dictionary entry the derivation reads. The execution will not
    /// commit, so its terms are released before the error is returned, once, as
    /// [`crate::ChangesetApplier::release_execution`] releases them: a caller releases nothing again.
    pub fn finish(self) -> Result<Executed, ChangesetError> {
        let quads = self.write.quads();
        let outcome = self.write.finish(None)?;
        let reads = reads_of(
            self.tenant,
            &outcome.request,
            &outcome.terms,
            &self.marks_read,
        );
        let fetched: BTreeSet<&[u8]> = outcome
            .request
            .read_keys()
            .chain(outcome.request.tested().map(|(key, _)| key))
            .chain(self.marks_read.iter().map(Vec::as_slice))
            .collect();
        let read: BTreeSet<Region> = fetched.iter().filter_map(|k| Region::of(k).ok()).collect();
        let lists_read = fetched.len();
        let mut request = outcome.request;
        for (key, value) in outcome.terms.entries() {
            request.add_versionless(key, value);
        }
        for marked in &self.marks {
            request.add_entry(mark_entry(self.tenant, marked));
        }
        let writes = request.into_writes();
        let referrers =
            match referrer_candidates(quads, self.tenant, self.read_ts, &writes, &outcome.terms) {
                Ok(referrers) => referrers,
                Err(e) => {
                    // The execution will not commit: its terms are released once, as a failed derivation's are.
                    quads.release(&outcome.terms);
                    return Err(e);
                }
            };
        Ok(Executed {
            writes,
            terms: outcome.terms,
            reads,
            read,
            lists_read,
            defaults: outcome.defaults,
            default_ids: outcome.default_ids,
            catalog_changed: outcome.catalog_changed,
            pending: outcome.pending,
            touched: outcome.touched,
            removals: outcome.removals,
            marks: self.marks,
            referrers,
            net: outcome.net,
            ..Executed::empty(self.tenant, self.read_ts, outcome.catalog)
        })
    }
}
