/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;

use ultragraphdb_core_codec::Posting;
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{Graph, QuadKey};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_posting::{
    BIT_DELTA_POSTING, CachedList, DEL, List, PostingError, Request, SET, Writes,
};
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{Entry, ListCache, Store};

use crate::errors::quad_error::QuadError;
use crate::types::quad_store::QuadStore;
use crate::types::statement_record::StatementRecord;
use crate::types::statistics::Statistics;
use crate::types::write_terms::WriteTerms;
use crate::utils::postings::{held_object, list_holds, object_posting, posting_object};

/// The statement IDs one object of a list holds, in a forward and a reverse list alike (`PROTOCOL.md` §11.9): what keeps a
/// key's object record inside a frame at the floor of `max_frame`.
pub const STATEMENTS_PER_OBJECT_MAX: usize = 16_000;

/// One write to the quads of a tenant: a request of the posting lists, the terms it names and the statistics it
/// changes. An insert writes the forward, reverse, subject-predicate and object-predicate lists without reading any,
/// a vector naming no reverse or object-predicate list; a delete reads the subject's list, so that it removes what is
/// held and keeps a predicate in the predicate lists while anything still holds it. [`QuadWrite::finish`] adds the
/// statistics deltas; [`QuadStore::commit`] commits the request.
#[derive(Debug)]
pub struct QuadWrite<'a, S, C, B> {
    quads: &'a QuadStore<S, C, B>,
    request: Request<'a, S, C>,
    terms: WriteTerms,
    stats: BTreeMap<(Graph, SystemId), Statistics>,
    inserted: BTreeSet<Vec<u8>>,
}

impl<'a, S, C, B> QuadWrite<'a, S, C, B>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    B: BlockSource,
{
    /// A write of tenant `tenant` to `quads` through `request`, resolving no term yet.
    pub fn new(quads: &'a QuadStore<S, C, B>, tenant: u64, request: Request<'a, S, C>) -> Self {
        Self {
            quads,
            request,
            terms: WriteTerms::new(tenant),
            stats: BTreeMap::new(),
            inserted: BTreeSet::new(),
        }
    }

    /// Continues the write `writes` and `terms` describe, a write finished before: its terms resolved as it resolved
    /// them, its list deltas the request's own blind writes, its statistics deltas merged into this write's, and its
    /// other entries, versionless writes, epoch mark and removed records added to the request. What a commit-time
    /// operation writes on top of its transaction with, so that the two commit as one write (7.5).
    ///
    /// # Errors
    ///
    /// [`QuadError::corrupted`] when a statistics delta of `writes` does not decode.
    pub fn seed(&mut self, writes: &Writes, terms: WriteTerms) -> Result<(), QuadError> {
        self.terms = terms;
        self.request.seed_deltas(writes.deltas());
        for entry in writes.entries() {
            match (QuadKey::parse(entry.key()), entry.value()) {
                (
                    Ok((
                        QuadKey::Statistics {
                            graph, predicate, ..
                        },
                        _,
                    )),
                    Some(value),
                ) => {
                    let delta = Statistics::decode(entry.key(), value)?;
                    self.stats
                        .entry((graph, predicate))
                        .or_default()
                        .merge(&delta);
                }
                _ => self.request.add_entry(entry.clone()),
            }
        }
        for entry in writes.versioned_entries() {
            self.request.add_entry_at_version(entry.clone());
        }
        for entry in writes.versionless() {
            self.request.add_versionless_entry(entry.clone());
        }
        if writes.advances_epoch() {
            self.request.advance_epoch_on_commit();
        }
        for &id in writes.removed_records() {
            self.request.remove_rebuild_record_on_commit(id);
        }
        Ok(())
    }

    /// The tenant the write writes.
    pub fn tenant(&self) -> u64 {
        self.terms.tenant()
    }

    /// The quad store the write writes to.
    #[must_use]
    pub fn quads(&self) -> &'a QuadStore<S, C, B> {
        self.quads
    }

    /// The request the write reads and writes through.
    pub fn request(&mut self) -> &mut Request<'a, S, C> {
        &mut self.request
    }

    /// The timestamp the write reads at.
    pub fn read_ts(&self) -> u64 {
        self.request.read_ts()
    }

    /// The terms the write names, to resolve more with.
    pub fn terms(&mut self) -> (&mut WriteTerms, &'a QuadStore<S, C, B>) {
        (&mut self.terms, self.quads)
    }

    /// Whether `id` was created by this write, so that no list keyed by it holds anything.
    #[must_use]
    pub fn is_fresh(&self, id: SystemId) -> bool {
        self.terms.is_fresh(id)
    }

    /// Inserts the statement `subject predicate object` in `graph`, its forward and reverse postings carrying
    /// `statement_ids`, reading no list. `kind` is the object's kind for the statistics: `iri`, `blank` or a literal's
    /// datatype IRI.
    pub fn insert(
        &mut self,
        graph: Graph,
        subject: SystemId,
        predicate: SystemId,
        object: &EncodedTerm,
        statement_ids: &[SystemId],
        kind: &str,
    ) {
        let fp = self.quads.fingerprint();
        let forward = self.forward(graph, predicate, subject);
        self.request
            .get_from_delta(&forward)
            .add_posting(object_posting(object, SET, statement_ids, fp), false);
        let tenant = self.tenant();
        let predicate_posting = || {
            Posting::default()
                .with_uid(predicate.to_u128())
                .with_op(SET)
        };
        let subject_predicates = QuadKey::SubjectPredicates {
            tenant,
            graph,
            subject,
        }
        .to_bytes();
        self.request
            .get_from_delta(&subject_predicates)
            .add_posting(predicate_posting(), false);
        if !object.is_vector() {
            let reverse = self.reverse(graph, predicate, object);
            self.request
                .get_from_delta(&reverse)
                .add_posting(subject_posting(subject, statement_ids), false);
            let object_predicates = QuadKey::ObjectPredicates {
                tenant,
                graph,
                object: object.clone(),
            }
            .to_bytes();
            self.request
                .get_from_delta(&object_predicates)
                .add_posting(predicate_posting(), false);
        }
        let mut quad = forward;
        quad.extend_from_slice(object.as_bytes());
        if self.inserted.insert(quad) {
            self.stats.entry((graph, predicate)).or_default().inserted(
                &subject.to_be_bytes(),
                object.as_bytes(),
                kind,
            );
        }
    }

    /// The objects of `subject`'s `predicate` in `graph`, as the write sees them, each with its statement IDs: a read
    /// of the subject's list, which none is for a subject the write created.
    pub fn objects(
        &mut self,
        graph: Graph,
        subject: SystemId,
        predicate: SystemId,
    ) -> Result<Vec<(EncodedTerm, Vec<SystemId>)>, QuadError> {
        let forward = self.forward(graph, predicate, subject);
        if self.terms.is_fresh(subject) {
            return read_list(&self.request.get_from_delta(&forward).clone(), &forward);
        }
        let list = self.request.get(&forward)?.clone();
        read_list(&list, &forward)
    }

    /// The objects of `subject`'s `predicate` in `graph` as the write's snapshot holds them, each with its statement
    /// IDs: without the write's own writes, and recorded as read nowhere. A subject the write created holds none.
    ///
    /// # Errors
    ///
    /// The store's error; corruption of the list.
    pub fn snapshot_objects(
        &self,
        graph: Graph,
        subject: SystemId,
        predicate: SystemId,
    ) -> Result<Vec<(EncodedTerm, Vec<SystemId>)>, QuadError> {
        if self.terms.is_fresh(subject) {
            return Ok(Vec::new());
        }
        let forward = self.forward(graph, predicate, subject);
        read_list(&self.request.snapshot(&forward)?, &forward)
    }

    /// Drops every list delta and statistics delta the write made, keeping its terms, its entries, its versionless
    /// writes and what it read: what a write whose keys are derived again from its net effect on its snapshot starts
    /// from.
    pub fn discard_lists(&mut self) {
        self.request.discard_deltas();
        self.stats.clear();
        self.inserted.clear();
    }

    /// The statement IDs of `subject predicate object` in `graph` when the subject holds it, as the write sees it,
    /// `None` when it does not: a read of the subject's list that records the statement it tests, not the list, so
    /// that an insert conflicts only with a commit that added or removed this object. None is recorded for a subject
    /// the write created.
    pub fn holds(
        &mut self,
        graph: Graph,
        subject: SystemId,
        predicate: SystemId,
        object: &EncodedTerm,
    ) -> Result<Option<Vec<SystemId>>, QuadError> {
        let forward = self.forward(graph, predicate, subject);
        let fingerprint = self.quads.fingerprint();
        let list = if self.terms.is_fresh(subject) {
            self.request.get_from_delta(&forward)
        } else {
            self.request.get_tested(&forward, object.as_bytes())?
        };
        Ok(held_object(list, object, fingerprint)?)
    }

    /// Deletes `subject predicate object` from `graph` when it is held: whether it was, and the statement IDs its
    /// posting carried. The predicate leaves the subject's predicate list when the subject holds no other object of it,
    /// and the object's when no other subject points at the object with it.
    pub fn delete(
        &mut self,
        graph: Graph,
        subject: SystemId,
        predicate: SystemId,
        object: &EncodedTerm,
        kind: &str,
    ) -> Result<Option<Vec<SystemId>>, QuadError> {
        let held = self.objects(graph, subject, predicate)?;
        let Some((_, ids)) = held.iter().find(|(o, _)| o == object) else {
            return Ok(None);
        };
        let ids = ids.clone();
        let fp = self.quads.fingerprint();
        let forward = self.forward(graph, predicate, subject);
        self.request
            .get(&forward)?
            .add_posting(object_posting(object, DEL, &[], fp), false);
        let predicate_delete = || {
            Posting::default()
                .with_uid(predicate.to_u128())
                .with_op(DEL)
        };
        if held.len() == 1 {
            let subject_predicates = QuadKey::SubjectPredicates {
                tenant: self.tenant(),
                graph,
                subject,
            }
            .to_bytes();
            self.request
                .get(&subject_predicates)?
                .add_posting(predicate_delete(), false);
        }
        if !object.is_vector() {
            self.delete_reverse(graph, subject, predicate, object)?;
        }
        let mut quad = forward;
        quad.extend_from_slice(object.as_bytes());
        let _was_inserted: bool = self.inserted.remove(&quad);
        self.stats
            .entry((graph, predicate))
            .or_default()
            .deleted(kind);
        Ok(Some(ids))
    }

    /// Rewrites the forward and reverse postings of a held object with `statement_ids`, as a parallel edge or a statement's
    /// first property changes them, reading no list: the predicate lists and the statistics are left as they are. A vector
    /// has no reverse posting.
    pub fn set_statement_ids(
        &mut self,
        graph: Graph,
        subject: SystemId,
        predicate: SystemId,
        object: &EncodedTerm,
        statement_ids: &[SystemId],
    ) {
        let fp = self.quads.fingerprint();
        let forward = self.forward(graph, predicate, subject);
        self.request
            .get_from_delta(&forward)
            .add_posting(object_posting(object, SET, statement_ids, fp), false);
        if !object.is_vector() {
            let reverse = self.reverse(graph, predicate, object);
            self.request
                .get_from_delta(&reverse)
                .add_posting(subject_posting(subject, statement_ids), false);
        }
    }

    /// The subjects that hold `object` for `predicate` in `graph`, as the write sees them: a read of the object's
    /// reverse list, which none is for an object the write created. A vector has none.
    pub fn subjects(
        &mut self,
        graph: Graph,
        predicate: SystemId,
        object: &EncodedTerm,
    ) -> Result<Vec<SystemId>, QuadError> {
        if object.is_vector() {
            return Ok(Vec::new());
        }
        let reverse = self.reverse(graph, predicate, object);
        let list = match object.as_id() {
            Some(id) if self.terms.is_fresh(id) => self.request.get_from_delta(&reverse).clone(),
            _ => self.request.get(&reverse)?.clone(),
        };
        Ok(read_list(&list, &reverse)?
            .iter()
            .filter_map(|(o, _)| o.as_id())
            .collect())
    }

    /// Whether `subject`'s subject-predicates list in `graph` names `predicate`, as the write sees it: the list read
    /// whole and recorded, so a commit giving the subject a predicate since conflicts (partitions, "Absence is proven by
    /// a node's predicate lists").
    ///
    /// # Errors
    ///
    /// The store's error; corruption of the list.
    pub fn names_predicate(
        &mut self,
        graph: Graph,
        subject: SystemId,
        predicate: SystemId,
    ) -> Result<bool, QuadError> {
        let key = QuadKey::SubjectPredicates {
            tenant: self.tenant(),
            graph,
            subject,
        }
        .to_bytes();
        self.list_names(&key, predicate)
    }

    /// Whether `object`'s object-predicates list in `graph` names `predicate`, as the write sees it: the list read
    /// whole and recorded.
    ///
    /// # Errors
    ///
    /// The store's error; corruption of the list.
    pub fn points_with(
        &mut self,
        graph: Graph,
        object: &EncodedTerm,
        predicate: SystemId,
    ) -> Result<bool, QuadError> {
        let key = QuadKey::ObjectPredicates {
            tenant: self.tenant(),
            graph,
            object: object.clone(),
        }
        .to_bytes();
        self.list_names(&key, predicate)
    }

    /// The IDs the list of `key` holds as the write sees it, in ID order: the list read whole and recorded, as a lookup
    /// reads a value-index list.
    ///
    /// # Errors
    ///
    /// The store's error; corruption of the list; [`QuadError::corrupted`] for a posting naming no system ID.
    pub fn ids(&mut self, key: &[u8]) -> Result<Vec<SystemId>, QuadError> {
        let list = self.request.get(key)?;
        let mut ids = Vec::new();
        let mut failure = None;
        list.iterate(list.mutation().read_ts(), 0, |p| {
            match SystemId::from_u128(p.uid()) {
                Ok(id) => ids.push(id),
                Err(e) => {
                    failure = Some(QuadError::corrupted(
                        key,
                        format!("a reference posting names no system ID: {e}"),
                    ));
                    return Ok(ControlFlow::Break(()));
                }
            }
            Ok::<_, PostingError>(ControlFlow::Continue(()))
        })?;
        match failure {
            Some(e) => Err(e),
            None => Ok(ids),
        }
    }

    /// Writes the record of statement `id` in `graph`, at the write's commit timestamp.
    pub fn put_statement(&mut self, graph: Graph, id: SystemId, record: &StatementRecord) {
        let key = self.statement_key(graph, id);
        self.request
            .add_entry(Entry::put(key, 0, 0, record.encode()));
    }

    /// Removes the record of statement `id` in `graph`.
    pub fn delete_statement(&mut self, graph: Graph, id: SystemId) {
        let key = self.statement_key(graph, id);
        self.request.add_entry(Entry::deletion(key, 0));
    }

    /// The record of statement `id` in `graph` at the write's read timestamp.
    ///
    /// # Errors
    ///
    /// The store's error; a stored record [`StatementRecord::decode`] refuses.
    pub fn statement(
        &self,
        graph: Graph,
        id: SystemId,
    ) -> Result<Option<StatementRecord>, QuadError> {
        self.quads
            .statement(self.tenant(), graph, id, self.read_ts())
    }

    /// A new statement ID of `subject`, named by a new reifier, as [`WriteTerms::statement`] gives it.
    ///
    /// # Errors
    ///
    /// As [`WriteTerms::statement`].
    pub fn next_statement_id(&mut self, subject: SystemId) -> Result<SystemId, QuadError> {
        let quads = self.quads;
        self.terms.statement(quads, subject)
    }

    /// The request and the terms to commit it with, the statistics deltas added to the request.
    ///
    /// # Errors
    ///
    /// As [`QuadWrite::add_statistics`] refuses.
    pub fn finish(mut self) -> Result<(Request<'a, S, C>, WriteTerms), QuadError> {
        self.add_statistics()?;
        Ok((self.request, self.terms))
    }

    /// Adds the statistics deltas the write holds to its request, each encoded, and holds none after; a caller that
    /// must release the write's terms when it is refused calls it before [`QuadWrite::finish`].
    ///
    /// # Errors
    ///
    /// [`QuadError::too_many_kinds`] or [`QuadError::kind_too_long`] for a delta its stored format cannot hold, as
    /// [`Statistics::encode`] refuses; nothing is added, and the deltas stay held.
    pub fn add_statistics(&mut self) -> Result<(), QuadError> {
        let tenant = self.tenant();
        let entries = self
            .stats
            .iter()
            .filter(|(_, delta)| !delta.is_empty())
            .map(|(&(graph, predicate), delta)| {
                let key = QuadKey::Statistics {
                    tenant,
                    graph,
                    predicate,
                }
                .to_bytes();
                Ok(Entry::put(key, 0, BIT_DELTA_POSTING, delta.encode()?))
            })
            .collect::<Result<Vec<_>, QuadError>>()?;
        self.stats.clear();
        for entry in entries {
            self.request.add_entry(entry);
        }
        Ok(())
    }

    /// Whether the predicate list of `key` names `predicate`, as the write sees it, read whole and recorded.
    fn list_names(&mut self, key: &[u8], predicate: SystemId) -> Result<bool, QuadError> {
        let list = self.request.get(key)?;
        Ok(list_holds(list, list.mutation().read_ts(), predicate)?)
    }

    fn forward(&self, graph: Graph, predicate: SystemId, subject: SystemId) -> Vec<u8> {
        QuadKey::Forward {
            tenant: self.tenant(),
            graph,
            predicate,
            subject,
        }
        .to_bytes()
    }

    fn reverse(&self, graph: Graph, predicate: SystemId, object: &EncodedTerm) -> Vec<u8> {
        QuadKey::Reverse {
            tenant: self.tenant(),
            graph,
            predicate,
            object: object.clone(),
        }
        .to_bytes()
    }

    fn statement_key(&self, graph: Graph, id: SystemId) -> Vec<u8> {
        QuadKey::Statement {
            tenant: self.tenant(),
            graph,
            statement: id,
        }
        .to_bytes()
    }

    /// Removes `subject` from `object`'s reverse list, and the predicate from the object's predicate list when no
    /// other subject points at the object with it.
    fn delete_reverse(
        &mut self,
        graph: Graph,
        subject: SystemId,
        predicate: SystemId,
        object: &EncodedTerm,
    ) -> Result<(), QuadError> {
        let reverse = self.reverse(graph, predicate, object);
        let list = self.request.get(&reverse)?;
        list.add_posting(
            Posting::default().with_uid(subject.to_u128()).with_op(DEL),
            false,
        );
        let remaining = list.clone();
        if read_list(&remaining, &reverse)?.is_empty() {
            let object_predicates = QuadKey::ObjectPredicates {
                tenant: self.tenant(),
                graph,
                object: object.clone(),
            }
            .to_bytes();
            self.request.get(&object_predicates)?.add_posting(
                Posting::default()
                    .with_uid(predicate.to_u128())
                    .with_op(DEL),
                false,
            );
        }
        Ok(())
    }
}

/// The posting setting `subject` in a reverse list, with the IDs of the statements holding it.
fn subject_posting(subject: SystemId, statement_ids: &[SystemId]) -> Posting {
    Posting::default()
        .with_uid(subject.to_u128())
        .with_op(SET)
        .with_statement_ids(statement_ids.to_vec())
}

/// The objects `list` holds at its own read timestamp, each with its statement IDs.
fn read_list(list: &List, key: &[u8]) -> Result<Vec<(EncodedTerm, Vec<SystemId>)>, QuadError> {
    let mut objects = Vec::new();
    let mut failure = None;
    list.iterate(list.mutation().read_ts(), 0, |p| {
        match posting_object(key, p) {
            Ok(object) => objects.push((object, p.statement_ids().to_vec())),
            Err(e) => {
                failure = Some(e);
                return Ok(ControlFlow::Break(()));
            }
        }
        Ok::<_, PostingError>(ControlFlow::Continue(()))
    })?;
    match failure {
        Some(e) => Err(e),
        None => Ok(objects),
    }
}
