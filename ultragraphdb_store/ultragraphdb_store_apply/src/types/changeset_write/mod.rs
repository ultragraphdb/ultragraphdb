/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod changeset_write_declare;
mod changeset_write_index;
mod changeset_write_statements;
mod changeset_write_triples;

use std::collections::BTreeMap;

use oxrdf::vocab::rdf;
use oxrdf::{
    BlankNode, GraphNameRef, Literal, NamedNode, NamedNodeRef, NamedOrBlankNodeRef, Term, TermRef,
};
use ultragraphdb_core_catalog::Catalog;
use ultragraphdb_core_mapping::{Change, Changeset, Declaration, PredicateDeclaration};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_posting::{CachedList, Request};
use ultragraphdb_store_quads::{QuadError, QuadErrorEnum, QuadWrite, StatementRecord, WriteTerms};
use ultragraphdb_traits_store::{Entry, ListCache, Store};

use crate::errors::changeset_error::ChangesetError;

/// The application of one changeset: its quad write, the catalog it is checked against, which its declarations and
/// the default entries of the predicates it names first change, and the statement records it writes, which later
/// changes of the same changeset read. Records and catalog entries are written once each, at [`ChangesetWrite::finish`],
/// so that a key the changeset writes twice lands once.
pub(crate) struct ChangesetWrite<'a, S, C> {
    write: QuadWrite<'a, S, C>,
    catalog: Catalog,
    /// The catalog entries the write adds, by key.
    entries: BTreeMap<Vec<u8>, Entry>,
    /// The statement records the write has written, or removed, by graph and ID.
    records: BTreeMap<(u64, u64), Option<StatementRecord>>,
}

impl<'a, S, C> ChangesetWrite<'a, S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    pub fn new(write: QuadWrite<'a, S, C>, catalog: Catalog) -> Self {
        Self {
            write,
            catalog,
            entries: BTreeMap::new(),
            records: BTreeMap::new(),
        }
    }

    /// Applies every change of `changeset`, in order; the first refusal ends it. A namespace with no catalog is given
    /// its initial one, written with the changeset.
    pub fn apply(&mut self, changeset: &Changeset) -> Result<(), ChangesetError> {
        if self.catalog.predicates().next().is_none() {
            let rdf_type = self.iri(rdf::TYPE)?;
            self.catalog = Catalog::initial(self.namespace(), rdf_type);
            self.catalog
                .entries(0)
                .into_iter()
                .for_each(|e| self.add_entry(e));
        }
        changeset
            .changes()
            .iter()
            .try_for_each(|change| self.change(change))
    }

    fn change(&mut self, change: &Change) -> Result<(), ChangesetError> {
        match change {
            Change::Insert {
                graph,
                subject,
                predicate,
                object,
            } => self.insert(
                graph.as_ref(),
                subject.as_ref(),
                predicate.as_ref(),
                object.as_ref(),
            ),
            Change::InsertEdge {
                graph,
                subject,
                predicate,
                object,
                properties,
            } => self.insert_edge(
                graph.as_ref(),
                subject.as_ref(),
                predicate.as_ref(),
                object.as_ref(),
                properties,
            ),
            Change::Delete {
                graph,
                subject,
                predicate,
                object,
            } => self.delete(
                graph.as_ref(),
                subject.as_ref(),
                predicate.as_ref(),
                object.as_ref(),
            ),
            Change::DeleteStatement { graph, id } => {
                self.delete_statement(graph.as_ref(), id.value())
            }
            Change::DeletePattern {
                graph,
                subject,
                predicate,
            } => self.delete_pattern(graph.as_ref(), subject.as_ref(), predicate.as_ref()),
            Change::SetProperty {
                statement,
                predicate,
                value,
            } => self.set_property(statement, predicate.as_ref(), value.as_ref()),
            Change::DeleteProperty {
                statement,
                predicate,
                value,
            } => self.delete_property(
                statement,
                predicate.as_ref(),
                value.as_ref().map(Term::as_ref),
            ),
            Change::Declare(declaration) => self.declare(declaration),
        }
    }

    /// The request, the terms to commit it with and the catalog its commit leaves, the statement records and catalog
    /// entries added.
    pub fn finish(mut self) -> (Request<'a, S, C>, WriteTerms, Catalog) {
        for ((graph, id), record) in std::mem::take(&mut self.records) {
            match record {
                Some(record) => self.write.put_statement(graph, id, &record),
                None => self.write.delete_statement(graph, id),
            }
        }
        let (mut request, terms) = self.write.finish();
        self.entries
            .into_values()
            .for_each(|e| request.add_entry(e));
        (request, terms, self.catalog)
    }

    fn namespace(&self) -> u64 {
        self.write.namespace()
    }

    fn add_entry(&mut self, entry: Entry) {
        self.entries.insert(entry.key().to_vec(), entry);
    }

    fn iri(&mut self, iri: NamedNodeRef<'_>) -> Result<u64, QuadError> {
        let (terms, quads) = self.write.terms();
        terms.iri(quads.store(), quads.leases(), iri.as_str())
    }

    fn graph(&mut self, graph: GraphNameRef<'_>) -> Result<u64, QuadError> {
        let (terms, quads) = self.write.terms();
        terms.graph(quads.store(), quads.leases(), graph)
    }

    fn node(&mut self, node: NamedOrBlankNodeRef<'_>) -> Result<u64, QuadError> {
        let (terms, quads) = self.write.terms();
        terms.node(quads.store(), quads.leases(), node)
    }

    /// The UID `iri` has, allocating none.
    fn find_iri(&mut self, iri: NamedNodeRef<'_>) -> Result<Option<u64>, QuadError> {
        let (terms, quads) = self.write.terms();
        terms.find_iri(quads.store(), iri.as_str())
    }

    /// The UID a graph has, allocating none; the default graph's is 0.
    fn find_graph(&mut self, graph: GraphNameRef<'_>) -> Result<Option<u64>, ChangesetError> {
        Ok(match graph {
            GraphNameRef::DefaultGraph => Some(ultragraphdb_core_keys::DEFAULT_GRAPH),
            GraphNameRef::NamedNode(n) => self.find_iri(n)?,
            GraphNameRef::BlankNode(b) => self
                .find_term(rdf::TYPE, b.into())?
                .and_then(|e| e.as_uid()),
        })
    }

    /// The encoding `term` has as an object or property value of `predicate`, allocating no UID: `None` when the
    /// store holds no term it names.
    fn find_term(
        &mut self,
        predicate: NamedNodeRef<'_>,
        term: TermRef<'_>,
    ) -> Result<Option<EncodedTerm>, ChangesetError> {
        let (terms, quads) = self.write.terms();
        terms.find(quads.store(), term).map_err(|e| match e.0 {
            QuadErrorEnum::Term(t) => ChangesetError::Literal(predicate.as_str(), t),
            _ => e.into(),
        })
    }

    /// The encoding of `term` as an object or property value of `predicate`: a literal that cannot be encoded, a
    /// vector out of its canonical form above all, is refused naming the predicate.
    fn term(
        &mut self,
        predicate: NamedNodeRef<'_>,
        term: TermRef<'_>,
    ) -> Result<EncodedTerm, ChangesetError> {
        let (terms, quads) = self.write.terms();
        terms
            .term(quads.store(), quads.leases(), term)
            .map_err(|e| match e.0 {
                QuadErrorEnum::Term(t) => ChangesetError::Literal(predicate.as_str(), t),
                _ => e.into(),
            })
    }

    /// The declaration predicate `uid` is written under: its own, or the default, which the write records in its
    /// commit batch the first time it names the predicate.
    fn declaration(
        &mut self,
        uid: u64,
        predicate: NamedNodeRef<'_>,
    ) -> Result<PredicateDeclaration, ChangesetError> {
        if let Some(declared) = self.catalog.predicate(uid) {
            return Ok(declared.clone());
        }
        let default = PredicateDeclaration::new(predicate.into_owned());
        self.catalog.declare_predicate(uid, default.clone())?;
        let entry = Catalog::entry(
            self.namespace(),
            uid,
            &Declaration::Predicate(default.clone()),
            0,
        );
        self.add_entry(entry);
        Ok(default)
    }

    /// The declaration predicate `uid` behaves as, recording none: what a delete reads.
    fn behaviour(&self, uid: u64, predicate: NamedNodeRef<'_>) -> PredicateDeclaration {
        self.catalog
            .predicate_or_default(uid, &predicate.into_owned())
            .into_owned()
    }

    /// The RDF term `encoded` names, the write's own new dictionary entries first; a skolemized blank node reads back
    /// as `_:b<uid>`.
    fn decode(&mut self, encoded: &EncodedTerm) -> Result<Term, ChangesetError> {
        let ns = self.namespace();
        let (terms, quads) = self.write.terms();
        let terms: &WriteTerms = terms;
        let iri = |uid: u64| -> Result<(String, bool), QuadError> {
            match terms.pending_iri(uid) {
                Some((iri, skolem)) => Ok((iri.to_string(), skolem)),
                None => quads.iri_of(ns, uid),
            }
        };
        if let Some(uid) = encoded.as_uid() {
            let (iri, skolem) = iri(uid)?;
            return Ok(if skolem {
                BlankNode::new_unchecked(format!("b{uid}")).into()
            } else {
                NamedNode::new_unchecked(iri).into()
            });
        }
        let literal: Literal = encoded.view().map_err(QuadError::from)?.to_literal(
            |uid| Ok::<_, QuadError>(NamedNode::new_unchecked(iri(uid)?.0)),
            |hash| match terms.pending_string(&hash) {
                Some(s) => Ok(s.to_string()),
                None => quads.string_of(ns, hash),
            },
        )?;
        Ok(literal.into())
    }

    /// The record of statement `id` in `graph`, as the write sees it.
    fn record(&self, graph: u64, id: u64) -> Result<Option<StatementRecord>, ChangesetError> {
        match self.records.get(&(graph, id)) {
            Some(record) => Ok(record.clone()),
            None => Ok(self.write.statement(graph, id)?),
        }
    }

    /// Marks predicate `uid` as carrying statement IDs, with its catalog entry when the mark is new.
    fn mark_statements(&mut self, uid: u64) {
        if self.catalog.mark_statements(uid) {
            let entry = Catalog::statements_entry(self.namespace(), uid, 0);
            self.add_entry(entry);
        }
    }
}

/// The statistics kind of `object`: `iri`, `blank`, or a literal's datatype IRI.
pub(crate) fn kind_of(object: TermRef<'_>) -> String {
    match object {
        TermRef::NamedNode(_) => "iri".to_string(),
        TermRef::BlankNode(_) => "blank".to_string(),
        TermRef::Literal(l) => l.datatype().as_str().to_string(),
        TermRef::Triple(_) => "triple".to_string(),
    }
}
