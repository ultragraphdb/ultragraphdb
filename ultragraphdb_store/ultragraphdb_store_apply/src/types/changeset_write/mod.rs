/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod changeset_write_data;
mod changeset_write_declare;
mod changeset_write_index;
mod changeset_write_operation;
mod changeset_write_read;
mod changeset_write_statements;
mod changeset_write_triples;
mod changeset_write_vector;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use oxrdf::vocab::rdf;
use oxrdf::{
    BlankNode, GraphNameRef, Literal, NamedNode, NamedNodeRef, NamedOrBlankNodeRef, Term, TermRef,
};
use ultragraphdb_core_catalog::Catalog;
use ultragraphdb_core_mapping::{Change, Changeset, Declaration, PredicateDeclaration};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_store_quads::{QuadError, QuadErrorEnum, QuadWrite, StatementRecord, WriteTerms};
use ultragraphdb_traits_store::{Entry, ListCache, Store};

use crate::errors::changeset_error::ChangesetError;
use crate::types::level_draw::LevelDraw;
use crate::types::predicate_prefix::PredicatePrefix;
use crate::types::rebuild::Rebuild;
use crate::types::write_outcome::WriteOutcome;

/// The application of one changeset: its quad write, the catalog it is checked against, which its declarations and
/// the default entries of the predicates it names first change, and the statement records it writes, which later
/// changes of the same changeset read. Records and catalog entries are written once each, at [`ChangesetWrite::finish`],
/// so that a key the changeset writes twice lands once. While an index is built, the predicate built is written under
/// the declaration the build maintains.
pub(crate) struct ChangesetWrite<'a, S, C> {
    write: QuadWrite<'a, S, C>,
    catalog: Catalog,
    /// The catalog entries the write adds, by key.
    entries: BTreeMap<Vec<u8>, Entry>,
    /// The keys of `entries` that are a predicate's implicit default entry and no declaration's, with the predicate.
    defaults: BTreeMap<Vec<u8>, u64>,
    /// Whether the write changes the catalog: a declaration, or the namespace's initial catalog.
    catalog_changed: bool,
    /// The statement records the write has written, or removed, by graph and ID.
    records: BTreeMap<(u64, u64), Option<StatementRecord>>,
    /// The predicate whose indexes are built and the declaration its writes are applied under.
    maintained: Option<(u64, PredicateDeclaration)>,
    /// The predicate whose vector index is built: writes record the subjects whose vector they change.
    vector_built: Option<u64>,
    /// Whether a build runs, or another changeset of the batch starts one: a declaration needing one is refused.
    building: bool,
    /// The build the write's declaration starts.
    pending: Option<Rebuild>,
    touched: BTreeSet<(u64, u64)>,
    /// The keys of the indexes the write's declarations remove.
    removals: Vec<PredicatePrefix>,
    /// The level draws of the vector index's inserts.
    draws: &'a Mutex<LevelDraw>,
}

impl<'a, S, C> ChangesetWrite<'a, S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    pub fn new(write: QuadWrite<'a, S, C>, catalog: Catalog, draws: &'a Mutex<LevelDraw>) -> Self {
        Self {
            write,
            catalog,
            entries: BTreeMap::new(),
            defaults: BTreeMap::new(),
            catalog_changed: false,
            records: BTreeMap::new(),
            maintained: None,
            vector_built: None,
            building: false,
            pending: None,
            touched: BTreeSet::new(),
            removals: Vec::new(),
            draws,
        }
    }

    /// The write while `rebuild` runs, or, with `None`, while another changeset of the batch starts one when
    /// `building` holds.
    pub fn under(mut self, rebuild: Option<&Rebuild>, building: bool) -> Self {
        if let Some(r) = rebuild.filter(|r| r.namespace == self.namespace()) {
            self.maintained = Some((r.predicate, r.maintained()));
            self.vector_built = r.plan.build_vector.then_some(r.predicate);
        }
        self.building = building || rebuild.is_some();
        self
    }

    /// Applies every change of `changeset`, in order; the first refusal ends it. A namespace with no catalog is given
    /// its initial one, written with the changeset.
    pub fn apply(&mut self, changeset: &Changeset) -> Result<(), ChangesetError> {
        self.initial()?;
        changeset
            .changes()
            .iter()
            .try_for_each(|change| self.change(change))
    }

    /// Gives a namespace with no catalog its initial one, written with the write.
    pub(crate) fn initial(&mut self) -> Result<(), ChangesetError> {
        if self.catalog.predicates().next().is_none() {
            let rdf_type = self.iri(rdf::TYPE)?;
            self.catalog = Catalog::initial(self.namespace(), rdf_type);
            self.catalog
                .entries(0)
                .into_iter()
                .for_each(|e| self.add_entry(e));
        }
        Ok(())
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

    /// What the write leaves to commit, the statement records and catalog entries added to its request.
    pub fn finish(mut self) -> WriteOutcome<'a, S, C> {
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
        WriteOutcome {
            request,
            terms,
            catalog: self.catalog,
            catalog_changed: self.catalog_changed,
            default_uids: self.defaults.values().copied().collect(),
            defaults: self.defaults.into_keys().collect(),
            pending: self.pending,
            touched: self.touched,
            removals: self.removals,
        }
    }

    fn namespace(&self) -> u64 {
        self.write.namespace()
    }

    /// Adds a catalog change's entry: a declaration's, or the namespace's initial catalog's.
    fn add_entry(&mut self, entry: Entry) {
        self.catalog_changed = true;
        self.defaults.remove(entry.key());
        self.entries.insert(entry.key().to_vec(), entry);
    }

    /// Adds predicate `uid`'s implicit default entry, an insert-if-absent on its key rather than a catalog change.
    fn add_default(&mut self, uid: u64, entry: Entry) {
        self.defaults.insert(entry.key().to_vec(), uid);
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
        if let Some((_, maintained)) = self.maintained.as_ref().filter(|(m, _)| *m == uid) {
            return Ok(maintained.clone());
        }
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
        self.add_default(uid, entry);
        Ok(default)
    }

    /// The declaration predicate `uid` behaves as, recording none: what a delete reads.
    fn behaviour(&self, uid: u64, predicate: NamedNodeRef<'_>) -> PredicateDeclaration {
        if let Some((_, maintained)) = self.maintained.as_ref().filter(|(m, _)| *m == uid) {
            return maintained.clone();
        }
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
