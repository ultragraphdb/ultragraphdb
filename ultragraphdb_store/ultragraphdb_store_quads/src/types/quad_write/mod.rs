/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;

use ultragraphdb_core_codec::Posting;
use ultragraphdb_core_keys::QuadKey;
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_posting::{BIT_DELTA_POSTING, CachedList, DEL, PostingError, Request, SET};
use ultragraphdb_traits_store::{Entry, ListCache, Store};

use crate::errors::quad_error::QuadError;
use crate::types::quad_store::QuadStore;
use crate::types::statement_record::StatementRecord;
use crate::types::statistics::Statistics;
use crate::types::write_terms::WriteTerms;
use crate::utils::postings::{object_posting, posting_object};

/// One write to the quads of a namespace: a request of the posting lists, the terms it names and the statistics it
/// changes. An insert writes the forward, reverse, subject-predicate and object-predicate lists without reading any,
/// a vector naming no reverse or object-predicate list; a delete reads the subject's list, so that it removes what is
/// held and keeps a predicate in the predicate lists while anything still holds it. [`QuadWrite::finish`] adds the
/// statistics deltas; [`QuadStore::commit`] commits the request.
#[derive(Debug)]
pub struct QuadWrite<'a, S, C> {
    quads: &'a QuadStore<S, C>,
    request: Request<'a, S, C>,
    terms: WriteTerms,
    stats: BTreeMap<(u64, u64), Statistics>,
    inserted: BTreeSet<Vec<u8>>,
}

impl<'a, S, C> QuadWrite<'a, S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    pub fn new(quads: &'a QuadStore<S, C>, namespace: u64, request: Request<'a, S, C>) -> Self {
        Self {
            quads,
            request,
            terms: WriteTerms::new(namespace),
            stats: BTreeMap::new(),
            inserted: BTreeSet::new(),
        }
    }

    pub fn namespace(&self) -> u64 {
        self.terms.namespace()
    }

    pub fn quads(&self) -> &'a QuadStore<S, C> {
        self.quads
    }

    pub fn request(&mut self) -> &mut Request<'a, S, C> {
        &mut self.request
    }

    pub fn read_ts(&self) -> u64 {
        self.request.read_ts()
    }

    /// The terms the write names, to resolve more with.
    pub fn terms(&mut self) -> (&mut WriteTerms, &'a QuadStore<S, C>) {
        (&mut self.terms, self.quads)
    }

    pub fn is_fresh(&self, uid: u64) -> bool {
        self.terms.is_fresh(uid)
    }

    fn forward(&self, graph: u64, predicate: u64, subject: u64) -> Vec<u8> {
        QuadKey::Forward {
            namespace: self.namespace(),
            graph,
            predicate,
            subject,
        }
        .to_bytes()
    }

    fn reverse(&self, graph: u64, predicate: u64, object: &EncodedTerm) -> Vec<u8> {
        QuadKey::Reverse {
            namespace: self.namespace(),
            graph,
            predicate,
            object: object.clone(),
        }
        .to_bytes()
    }

    /// Inserts the statement `subject predicate object` in `graph`, the object's posting carrying `statement_ids`,
    /// reading no list. `kind` is the object's kind for the statistics: `iri`, `blank` or a literal's datatype IRI.
    pub fn insert(
        &mut self,
        graph: u64,
        subject: u64,
        predicate: u64,
        object: &EncodedTerm,
        statement_ids: &[u64],
        kind: &str,
    ) {
        let fp = self.quads.fingerprint();
        let forward = self.forward(graph, predicate, subject);
        self.request
            .get_from_delta(&forward)
            .add_posting(object_posting(object, SET, statement_ids, fp), false);
        let ns = self.namespace();
        let predicate_posting = || Posting::default().with_uid(predicate).with_op(SET);
        let subject_predicates = QuadKey::SubjectPredicates {
            namespace: ns,
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
                .add_posting(Posting::default().with_uid(subject).with_op(SET), false);
            let object_predicates = QuadKey::ObjectPredicates {
                namespace: ns,
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
        graph: u64,
        subject: u64,
        predicate: u64,
    ) -> Result<Vec<(EncodedTerm, Vec<u64>)>, QuadError> {
        if self.terms.is_fresh(subject) {
            let forward = self.forward(graph, predicate, subject);
            return read_list(&self.request.get_from_delta(&forward).clone(), &forward);
        }
        let forward = self.forward(graph, predicate, subject);
        let list = self.request.get(&forward)?.clone();
        read_list(&list, &forward)
    }

    /// The statement IDs of `subject predicate object` in `graph` when the subject holds it, as the write sees it,
    /// `None` when it does not: a read of the subject's list that records the statement it tests, not the list, so
    /// that an insert conflicts only with a commit that added or removed this object. None is recorded for a subject
    /// the write created.
    pub fn holds(
        &mut self,
        graph: u64,
        subject: u64,
        predicate: u64,
        object: &EncodedTerm,
    ) -> Result<Option<Vec<u64>>, QuadError> {
        let forward = self.forward(graph, predicate, subject);
        let list = if self.terms.is_fresh(subject) {
            self.request.get_from_delta(&forward).clone()
        } else {
            self.request
                .get_tested(&forward, object.as_bytes())?
                .clone()
        };
        Ok(read_list(&list, &forward)?
            .into_iter()
            .find(|(o, _)| o == object)
            .map(|(_, ids)| ids))
    }

    /// Deletes `subject predicate object` from `graph` when it is held: whether it was, and the statement IDs its
    /// posting carried. The predicate leaves the subject's predicate list when the subject holds no other object of it,
    /// and the object's when no other subject points at the object with it.
    pub fn delete(
        &mut self,
        graph: u64,
        subject: u64,
        predicate: u64,
        object: &EncodedTerm,
        kind: &str,
    ) -> Result<Option<Vec<u64>>, QuadError> {
        let held = self.objects(graph, subject, predicate)?;
        let Some((_, ids)) = held.iter().find(|(o, _)| o == object) else {
            return Ok(None);
        };
        let ids = ids.clone();
        let fp = self.quads.fingerprint();
        let ns = self.namespace();
        let forward = self.forward(graph, predicate, subject);
        self.request
            .get(&forward)?
            .add_posting(object_posting(object, DEL, &[], fp), false);
        let predicate_delete = || Posting::default().with_uid(predicate).with_op(DEL);
        if held.len() == 1 {
            let subject_predicates = QuadKey::SubjectPredicates {
                namespace: ns,
                graph,
                subject,
            }
            .to_bytes();
            self.request
                .get(&subject_predicates)?
                .add_posting(predicate_delete(), false);
        }
        if !object.is_vector() {
            let reverse = self.reverse(graph, predicate, object);
            let list = self.request.get(&reverse)?;
            list.add_posting(Posting::default().with_uid(subject).with_op(DEL), false);
            let remaining = list.clone();
            if read_list(&remaining, &reverse)?.is_empty() {
                let object_predicates = QuadKey::ObjectPredicates {
                    namespace: ns,
                    graph,
                    object: object.clone(),
                }
                .to_bytes();
                self.request
                    .get(&object_predicates)?
                    .add_posting(predicate_delete(), false);
            }
        }
        let mut quad = forward;
        quad.extend_from_slice(object.as_bytes());
        self.inserted.remove(&quad);
        self.stats
            .entry((graph, predicate))
            .or_default()
            .deleted(kind);
        Ok(Some(ids))
    }

    /// Rewrites the forward posting of a held object with `statement_ids`, as a parallel edge or a statement's first
    /// property changes them: the reverse and predicate lists and the statistics are left as they are.
    pub fn set_statement_ids(
        &mut self,
        graph: u64,
        subject: u64,
        predicate: u64,
        object: &EncodedTerm,
        statement_ids: &[u64],
    ) {
        let fp = self.quads.fingerprint();
        let forward = self.forward(graph, predicate, subject);
        self.request
            .get_from_delta(&forward)
            .add_posting(object_posting(object, SET, statement_ids, fp), false);
    }

    /// The subjects that hold `object` for `predicate` in `graph`, as the write sees them: a read of the object's
    /// reverse list, which none is for an object the write created. A vector has none.
    pub fn subjects(
        &mut self,
        graph: u64,
        predicate: u64,
        object: &EncodedTerm,
    ) -> Result<Vec<u64>, QuadError> {
        if object.is_vector() {
            return Ok(Vec::new());
        }
        let reverse = self.reverse(graph, predicate, object);
        let list = match object.as_uid() {
            Some(uid) if self.terms.is_fresh(uid) => self.request.get_from_delta(&reverse).clone(),
            _ => self.request.get(&reverse)?.clone(),
        };
        Ok(read_list(&list, &reverse)?
            .iter()
            .filter_map(|(o, _)| o.as_uid())
            .collect())
    }

    /// Writes the record of statement `id` in `graph`, at the write's commit timestamp.
    pub fn put_statement(&mut self, graph: u64, id: u64, record: &StatementRecord) {
        let key = QuadKey::Statement {
            namespace: self.namespace(),
            graph,
            statement: id,
        }
        .to_bytes();
        self.request
            .add_entry(Entry::put(key, 0, 0, record.encode()));
    }

    /// Removes the record of statement `id` in `graph`.
    pub fn delete_statement(&mut self, graph: u64, id: u64) {
        let key = QuadKey::Statement {
            namespace: self.namespace(),
            graph,
            statement: id,
        }
        .to_bytes();
        self.request.add_entry(Entry::deletion(key, 0));
    }

    /// The record of statement `id` in `graph` at the write's read timestamp.
    pub fn statement(&self, graph: u64, id: u64) -> Result<Option<StatementRecord>, QuadError> {
        self.quads
            .statement(self.namespace(), graph, id, self.read_ts())
    }

    /// The next statement ID of the namespace.
    pub fn next_statement_id(&self) -> Result<u64, QuadError> {
        self.quads
            .leases()
            .next_statement_id(self.quads.store(), self.namespace())
    }

    /// The request and the terms to commit it with, the statistics deltas added to the request.
    pub fn finish(mut self) -> (Request<'a, S, C>, WriteTerms) {
        let ns = self.namespace();
        for ((graph, predicate), delta) in std::mem::take(&mut self.stats) {
            if !delta.is_empty() {
                let key = QuadKey::Statistics {
                    namespace: ns,
                    graph,
                    predicate,
                }
                .to_bytes();
                self.request
                    .add_entry(Entry::put(key, 0, BIT_DELTA_POSTING, delta.encode()));
            }
        }
        (self.request, self.terms)
    }
}

/// The objects `list` holds at its own read timestamp.
fn read_list(
    list: &ultragraphdb_store_posting::List,
    key: &[u8],
) -> Result<Vec<(EncodedTerm, Vec<u64>)>, QuadError> {
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
