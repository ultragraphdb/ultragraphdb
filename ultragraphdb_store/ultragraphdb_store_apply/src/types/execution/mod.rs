/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use oxrdf::{GraphNameRef, NamedNode, NamedNodeRef, NamedOrBlankNodeRef, Term};
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::Region;
use ultragraphdb_core_mapping::Changeset;
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{ListCache, Store};

use ultragraphdb_store_quads::WriteTerms;

use crate::errors::changeset_error::ChangesetError;
use crate::types::changeset_core::reads_of;
use crate::types::changeset_write::ChangesetWrite;
use crate::types::executed::Executed;

/// A transaction's execution held open across its requests: one request of the posting lists at its read version,
/// which every changeset applied to it writes and every read reads, so a read sees the execution's own writes on top
/// of its snapshot and nothing another execution wrote. [`Execution::finish`] gives the [`Executed`] a commit writes,
/// its keys derived once from its net effect on its snapshot; dropping it writes nothing and closes its read.
///
/// The changesets applied to one execution behave as one changeset holding their changes in order.
pub struct Execution<'a, S, C, B> {
    namespace: u64,
    read_ts: u64,
    write: ChangesetWrite<'a, S, C, B>,
}

impl<S, C, B> std::fmt::Debug for Execution<'_, S, C, B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Execution")
            .field("namespace", &self.namespace)
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
    pub(crate) fn new(namespace: u64, read_ts: u64, write: ChangesetWrite<'a, S, C, B>) -> Self {
        Self {
            namespace,
            read_ts,
            write,
        }
    }

    /// The namespace the execution writes.
    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    /// The version the execution reads at.
    pub fn read_ts(&self) -> u64 {
        self.read_ts
    }

    /// Applies every change of `changeset`, in order, on top of what the execution wrote before; the first refusal
    /// ends it, and the execution should then be dropped, since the changes before the refusal stay applied to it.
    ///
    /// # Errors
    ///
    /// The changeset's refusal. The changeset's namespace is not checked: its changes are written in the execution's.
    pub fn apply(&mut self, changeset: &Changeset) -> Result<(), ChangesetError> {
        self.write.apply(changeset)
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
        let outcome = self.write.finish(None)?;
        let reads = reads_of(self.namespace, &outcome.request, &outcome.terms);
        let fetched: BTreeSet<&[u8]> = outcome
            .request
            .read_keys()
            .chain(outcome.request.tested().map(|(key, _)| key))
            .collect();
        let read: BTreeSet<Region> = fetched.iter().filter_map(|k| Region::of(k).ok()).collect();
        let lists_read = fetched.len();
        let mut request = outcome.request;
        for (key, value) in outcome.terms.entries() {
            request.add_versionless(key, value);
        }
        Ok(Executed {
            writes: request.into_writes(),
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
            ..Executed::empty(self.namespace, self.read_ts, outcome.catalog)
        })
    }
}
