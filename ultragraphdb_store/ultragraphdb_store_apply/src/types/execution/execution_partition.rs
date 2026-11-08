/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU64;

use oxrdf::{GraphNameRef, NamedNodeRef, NamedOrBlankNode, NamedOrBlankNodeRef, TermRef};
use ultragraphdb_core_ids::{BucketGroup, SystemId};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{ListCache, Store};

use crate::errors::changeset_error::ChangesetError;
use crate::types::execution::Execution;
use crate::types::marked_reference::MarkedReference;
use crate::utils::mark_keys::mark_key;

/// What an execution does as one partition's (partitions; term-dictionary, "A reference to another partition's entity
/// registers at its home"): its reverse reads and index lookups, the IDs and marks of other partitions' entities it
/// was given by their homes, and the unique values it writes, which the transaction checks against the bucket owners.
impl<S, C, B> Execution<'_, S, C, B>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    B: BlockSource,
{
    /// The subjects holding `predicate` to `object` in `graph` as the execution sees them, into `out`, cleared first:
    /// the partition's own reverse list of `object` read whole on top of the execution's writes and recorded in the
    /// read set, after `object`'s object-predicates list, which is recorded too and names the predicates whose lists
    /// are read (partitions, "Absence is proven by a node's predicate lists").
    ///
    /// # Errors
    ///
    /// The store's error; corruption of a list or a dictionary entry.
    pub fn subjects_into(
        &mut self,
        graph: GraphNameRef<'_>,
        predicate: NamedNodeRef<'_>,
        object: TermRef<'_>,
        out: &mut Vec<NamedOrBlankNode>,
    ) -> Result<(), ChangesetError> {
        self.write.subjects_into(graph, predicate, object, out)
    }

    /// The subjects whose `predicate` in `graph` holds `value` by the predicate's `tokenizer` index, as the execution
    /// sees them, into `out`, cleared first, sorted: the index's keys of `value`'s tokens read and recorded in the read
    /// set.
    ///
    /// # Errors
    ///
    /// A refusal naming `predicate` and `tokenizer` when the catalog declares no such index on the predicate; the
    /// store's error; corruption of a list or a dictionary entry.
    pub fn lookup_into(
        &mut self,
        graph: GraphNameRef<'_>,
        predicate: NamedNodeRef<'_>,
        tokenizer: &str,
        value: TermRef<'_>,
        out: &mut Vec<NamedOrBlankNode>,
    ) -> Result<(), ChangesetError> {
        self.write
            .lookup_into(graph, predicate, tokenizer, value, out)
    }

    /// The ID of `node` as the execution resolves it when a change names it: an IRI's, a blank node's by its label, new
    /// on its first use and the same on every later one.
    ///
    /// # Errors
    ///
    /// The dictionary's refusals of creating an ID; the store's error.
    pub fn node_id(&mut self, node: NamedOrBlankNodeRef<'_>) -> Result<SystemId, ChangesetError> {
        Ok(self.write.node(node)?)
    }

    /// Takes `iri` as named by `id`, the ID its home answered a registration with: the execution resolves `iri` to
    /// `id` and its commit copies `iri`'s dictionary entries in both directions into the partition's dictionary.
    pub fn adopt(&mut self, iri: &str, id: SystemId) {
        self.write.adopt(iri, id);
    }

    /// The generation of the mark of `(entity, group)` the partition holds, as the execution sees it, recorded in the
    /// read set; `None` when it holds none.
    ///
    /// # Errors
    ///
    /// The store's error; corruption of a mark.
    pub fn read_mark(
        &mut self,
        entity: SystemId,
        group: BucketGroup,
    ) -> Result<Option<NonZeroU64>, ChangesetError> {
        self.record_mark(entity, group);
        if let Some(marked) = self
            .marks
            .iter()
            .find(|m| m.entity() == entity && m.group() == group)
        {
            return Ok(Some(marked.generation()));
        }
        Ok(self
            .write
            .quads()
            .mark(self.tenant, entity, group, self.read_ts)?)
    }

    /// Marks `(entity, group)` at `generation`, the generation a home's reply named: the mark's key is read into the
    /// read set, so a renewal committed after the reply conflicts with the transaction (partitions, "A commit under a
    /// registration reply reads its mark"), and written at `generation` by its commit, which lists the registration
    /// ([`crate::Executed::marks`]).
    ///
    /// # Errors
    ///
    /// The store's error reading the mark.
    pub fn mark(
        &mut self,
        entity: SystemId,
        group: BucketGroup,
        generation: NonZeroU64,
    ) -> Result<(), ChangesetError> {
        self.record_mark(entity, group);
        let marked = MarkedReference::new(entity, group, generation);
        match self
            .marks
            .iter_mut()
            .find(|m| m.entity() == entity && m.group() == group)
        {
            Some(held) => *held = marked,
            None => self.marks.push(marked),
        }
        Ok(())
    }

    /// Every value of a unique predicate the execution's changesets wrote by a plain write, with the predicate's ID,
    /// as the bucket derivation hashes it (system-identity, "The bucket comes from the tenant and the key"):
    /// `(predicate, value's term encoding)`, in the order written.
    #[must_use]
    pub fn unique_writes(&self) -> &[(SystemId, Vec<u8>)] {
        self.write.unique_writes()
    }

    /// Records the mark key of `(entity, group)` as read, once.
    fn record_mark(&mut self, entity: SystemId, group: BucketGroup) {
        let key = mark_key(self.tenant, entity, group);
        if !self.marks_read.contains(&key) {
            self.marks_read.push(key);
        }
    }
}
