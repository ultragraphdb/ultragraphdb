/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod changeset_write_data;
mod changeset_write_declare;
mod changeset_write_index;
mod changeset_write_logical;
mod changeset_write_operation;
mod changeset_write_read;
mod changeset_write_statements;
mod changeset_write_triples;
mod changeset_write_vector;

use std::collections::{BTreeMap, BTreeSet};

use oxrdf::vocab::rdf;
use oxrdf::{GraphNameRef, Literal, NamedNode, NamedNodeRef, NamedOrBlankNodeRef, Term, TermRef};
use ultragraphdb_core_catalog::{Catalog, PREDICATE};
use ultragraphdb_core_codec::Posting;
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{CountDirection, Graph};
use ultragraphdb_core_mapping::{Change, Changeset, PredicateDeclaration};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_store_quads::{
    QuadError, QuadErrorEnum, QuadWrite, StatementRecord, WriteTerms, skolem_blank_node,
};
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{ListCache, Store};

use crate::errors::changeset_error::ChangesetError;
use crate::types::predicate_prefix::PredicatePrefix;
use crate::types::rebuild::Rebuild;
use crate::types::write_outcome::WriteOutcome;
use crate::utils::catalog_keys::catalog_key;

/// The count moves a write records, by count list and member, each the member's identity: from the count before the
/// write's first move, to the count of its last, with the member's posting.
type CountMoves = BTreeMap<(Graph, SystemId, CountDirection, Vec<u8>), (u32, u32, Posting)>;

/// The application of one changeset: its quad write, the catalog it is checked against, which its declarations and
/// the default entries of the predicates its derived statements name first change, and the statement records it
/// writes, which later changes of the same changeset read. Records and catalog entries are written once each, at
/// [`ChangesetWrite::finish`], so that a key the changeset writes twice lands once. While an index is built, the
/// predicate built is written under the declaration the build maintains.
///
/// Its changes resolve the changeset's semantics against the write's own forward and reverse lists, which they read,
/// and maintain no value index, count index or vector record. [`ChangesetWrite::finish`] then derives every key of
/// the statements once, from the write's net effect on its snapshot, with [`ChangesetWrite::derive`], the derivation
/// replay runs on a logged record, so the first apply and a replay write the same keys and values (write-ahead-log,
/// "A round's record is logical").
pub(crate) struct ChangesetWrite<'a, S, C, B> {
    write: QuadWrite<'a, S, C, B>,
    catalog: Catalog,
    /// The keys of `entries` that are a predicate's implicit default entry and no declaration's, with the predicate.
    defaults: BTreeMap<Vec<u8>, SystemId>,
    /// Whether the write changes the catalog: a declaration, or the namespace's initial catalog.
    catalog_changed: bool,
    /// The statement records the write has written, or removed, by graph and ID.
    records: BTreeMap<(Graph, SystemId), Option<StatementRecord>>,
    /// The predicate whose indexes are built and the declaration its writes are applied under.
    maintained: Option<(SystemId, PredicateDeclaration)>,
    /// The predicate whose vector index is built: writes record the subjects whose vector they change.
    vector_built: Option<SystemId>,
    /// Whether a build runs, or another changeset of the batch starts one: a declaration needing one is refused.
    building: bool,
    /// The build the write's declaration starts.
    pending: Option<Rebuild>,
    touched: BTreeSet<(Graph, SystemId)>,
    /// The keys of the indexes the write's declarations remove.
    removals: Vec<PredicatePrefix>,
    /// Whether the write derives keys: adds and removals then maintain the value and count indexes and record the
    /// vectors they change.
    deriving: bool,
    /// The count moves a derivation records, written once at its end.
    counts: CountMoves,
}

impl<'a, S, C, B> ChangesetWrite<'a, S, C, B>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    B: BlockSource,
{
    pub fn new(write: QuadWrite<'a, S, C, B>, catalog: Catalog) -> Self {
        Self {
            write,
            catalog,
            defaults: BTreeMap::new(),
            catalog_changed: false,
            records: BTreeMap::new(),
            maintained: None,
            vector_built: None,
            building: false,
            pending: None,
            touched: BTreeSet::new(),
            removals: Vec::new(),
            deriving: false,
            counts: BTreeMap::new(),
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
            let version = self.catalog.version();
            self.catalog = Catalog::initial(self.namespace(), rdf_type).with_version(version);
            self.add_entry(&catalog_key(self.namespace(), PREDICATE, rdf_type));
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
            Change::DeleteStatement { graph, reifier } => {
                self.delete_statement(graph.as_ref(), reifier.as_ref())
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

    /// What the write leaves to commit, its keys derived once from its net effect on its snapshot: every list and
    /// statistics delta its changes wrote is dropped, keeping what they read, and [`ChangesetWrite::derive`] writes the
    /// statements they left removed or inserted, as replay writes a logged record's; the statement records and
    /// catalog entries are added to its request.
    ///
    /// # Errors
    ///
    /// The store's error, or the corruption of a list or dictionary entry the derivation reads. The write will not
    /// commit, so before the error is returned what it holds is released once, as [`ChangesetWrite::release`] does with
    /// `base`: every hold of a write of its own (`None`), or those it took beyond `base`, the terms it continues.
    pub fn finish(
        mut self,
        base: Option<&WriteTerms>,
    ) -> Result<WriteOutcome<'a, S, C>, ChangesetError> {
        if let Err(e) = self.derive_net() {
            self.release(base);
            return Err(e);
        }
        Ok(self.outcome())
    }

    /// Derives the write's keys from its net effect, every list and statistics delta its changes wrote dropped.
    fn derive_net(&mut self) -> Result<(), ChangesetError> {
        let net = self.net_writes()?;
        self.write.discard_lists();
        self.derive(&net)
    }

    /// Releases what the write holds, writing nothing: every pending entry it created or took with `base` `None`, and
    /// with `base`, the terms a write continuing another started from, only those it took beyond them, which leaves
    /// the other write's holds with it.
    pub(crate) fn release(&mut self, base: Option<&WriteTerms>) {
        let (terms, quads) = self.write.terms();
        match base {
            None => quads.release(terms),
            Some(base) => quads.release_beyond(terms, base),
        }
    }

    /// What the write leaves to commit as it is: its count moves written, and the statement records added to its
    /// request. Its catalog entries reach no store: a partition holds no catalog key, and the catalog the write leaves
    /// is installed in memory once its round is written (schema-storage, "The schema persists in the metadata group").
    pub fn outcome(mut self) -> WriteOutcome<'a, S, C> {
        self.write_counts();
        for ((graph, id), record) in std::mem::take(&mut self.records) {
            match record {
                Some(record) => self.write.put_statement(graph, id, &record),
                None => self.write.delete_statement(graph, id),
            }
        }
        let (request, terms) = self.write.finish();
        WriteOutcome {
            request,
            terms,
            catalog: self.catalog,
            catalog_changed: self.catalog_changed,
            default_ids: self.defaults.values().copied().collect(),
            defaults: self.defaults.into_keys().collect(),
            pending: self.pending,
            touched: self.touched,
            removals: self.removals,
        }
    }

    fn namespace(&self) -> u64 {
        self.write.namespace()
    }

    /// The terms the write resolved so far.
    pub(crate) fn terms(&mut self) -> &WriteTerms {
        self.write.terms().0
    }

    /// Records a catalog change under its key: a declaration's, or the namespace's initial catalog's.
    fn add_entry(&mut self, key: &[u8]) {
        self.catalog_changed = true;
        let _removed: Option<SystemId> = self.defaults.remove(key);
    }

    /// Records predicate `id`'s implicit default entry under its key, no catalog change.
    fn add_default(&mut self, id: SystemId, key: Vec<u8>) {
        let _previous_default: Option<SystemId> = self.defaults.insert(key, id);
    }

    fn iri(&mut self, iri: NamedNodeRef<'_>) -> Result<SystemId, QuadError> {
        let (terms, quads) = self.write.terms();
        terms.iri(quads, iri.as_str())
    }

    fn graph(&mut self, graph: GraphNameRef<'_>) -> Result<Graph, QuadError> {
        let (terms, quads) = self.write.terms();
        terms.graph(quads, graph)
    }

    fn node(&mut self, node: NamedOrBlankNodeRef<'_>) -> Result<SystemId, QuadError> {
        let (terms, quads) = self.write.terms();
        terms.node(quads, node)
    }

    /// The ID `iri` has as a delete or a read names it: got or created, as every IRI is, so a term no statement holds
    /// is recorded absent by its ID; `None` for an IRI under the skolem base the dictionary does not hold.
    fn find_iri(&mut self, iri: NamedNodeRef<'_>) -> Result<Option<SystemId>, ChangesetError> {
        Ok(self
            .find_term(rdf::TYPE, iri.into())?
            .and_then(|e| e.as_id()))
    }

    /// The graph a graph name names, as a delete or a read resolves it: `None` for a blank node the write did not name
    /// and an unheld skolem IRI.
    fn find_graph(&mut self, graph: GraphNameRef<'_>) -> Result<Option<Graph>, ChangesetError> {
        Ok(match graph {
            GraphNameRef::DefaultGraph => Some(Graph::Default),
            GraphNameRef::NamedNode(n) => self.find_iri(n)?.map(Graph::Named),
            GraphNameRef::BlankNode(b) => self
                .find_term(rdf::TYPE, b.into())?
                .and_then(|e| e.as_id())
                .map(Graph::Named),
        })
    }

    /// The encoding `term` has as an object or property value of `predicate`, as a delete or a read resolves it:
    /// `None` when no statement can hold it.
    fn find_term(
        &mut self,
        predicate: NamedNodeRef<'_>,
        term: TermRef<'_>,
    ) -> Result<Option<EncodedTerm>, ChangesetError> {
        let (terms, quads) = self.write.terms();
        terms
            .find(quads, term)
            .map_err(|e| literal_refusal(predicate, e))
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
            .term(quads, term)
            .map_err(|e| literal_refusal(predicate, e))
    }

    /// The declaration predicate `uid` is written under: its own, or the default. Only the derivation records the
    /// default, in the catalog and the commit batch, the first time it names the predicate: a change names a predicate
    /// its net effect may not keep, and replay derives the net effect alone, so the predicates of the statements
    /// derived are the ones whose default entry the first apply and a replay both write.
    fn declaration(
        &mut self,
        uid: SystemId,
        predicate: NamedNodeRef<'_>,
    ) -> Result<PredicateDeclaration, ChangesetError> {
        if let Some((_, maintained)) = self.maintained.as_ref().filter(|(m, _)| *m == uid) {
            return Ok(maintained.clone());
        }
        if let Some(declared) = self.catalog.predicate(uid) {
            return Ok(declared.clone());
        }
        let default = PredicateDeclaration::new(predicate.into_owned());
        if !self.deriving {
            return Ok(default);
        }
        self.catalog.declare_predicate(uid, default.clone())?;
        self.add_default(uid, catalog_key(self.namespace(), PREDICATE, uid));
        Ok(default)
    }

    /// The declaration predicate `uid` behaves as, recording none: what a delete reads.
    fn behaviour(&self, uid: SystemId, predicate: NamedNodeRef<'_>) -> PredicateDeclaration {
        if let Some((_, maintained)) = self.maintained.as_ref().filter(|(m, _)| *m == uid) {
            return maintained.clone();
        }
        self.catalog
            .predicate_or_default(uid, &predicate.into_owned())
            .into_owned()
    }

    /// The RDF term `encoded` names, the write's own new dictionary entries first; a skolemized blank node reads back
    /// as a blank node labelled by its skolem IRI, as the quad store labels it.
    fn decode(&mut self, encoded: &EncodedTerm) -> Result<Term, ChangesetError> {
        let ns = self.namespace();
        let (terms, quads) = self.write.terms();
        let terms: &WriteTerms = terms;
        let iri = |id: SystemId| -> Result<(String, bool), QuadError> {
            match terms.pending_iri(id) {
                Some((iri, skolem)) => Ok((iri.to_string(), skolem)),
                None => quads.iri_of(ns, id),
            }
        };
        if let Some(id) = encoded.as_id() {
            let (iri, skolem) = iri(id)?;
            return Ok(if skolem {
                skolem_blank_node(&iri).into()
            } else {
                NamedNode::new_unchecked(iri).into()
            });
        }
        let literal: Literal = encoded.view().map_err(QuadError::from)?.to_literal(
            |id| Ok::<_, QuadError>(NamedNode::new_unchecked(iri(id)?.0)),
            |hash| match terms.pending_string(&hash) {
                Some(s) => Ok(s.to_string()),
                None => quads.string_of(ns, hash),
            },
        )?;
        Ok(literal.into())
    }

    /// The record of statement `id` in `graph`, as the write sees it.
    fn record(
        &self,
        graph: Graph,
        id: SystemId,
    ) -> Result<Option<StatementRecord>, ChangesetError> {
        match self.records.get(&(graph, id)) {
            Some(record) => Ok(record.clone()),
            None => Ok(self.write.statement(graph, id)?),
        }
    }
}

/// A literal the quad store could not encode, refused naming `predicate`; any other refusal as it is.
fn literal_refusal(predicate: NamedNodeRef<'_>, e: QuadError) -> ChangesetError {
    match e.0 {
        QuadErrorEnum::Term(t) => ChangesetError::literal(predicate.as_str(), t),
        QuadErrorEnum::Store(_)
        | QuadErrorEnum::Posting(_)
        | QuadErrorEnum::Key(_)
        | QuadErrorEnum::Mapping(_)
        | QuadErrorEnum::Catalog(_)
        | QuadErrorEnum::Id(_)
        | QuadErrorEnum::Corrupted { .. }
        | QuadErrorEnum::NoSequence { .. }
        | QuadErrorEnum::PendingFull { .. }
        | QuadErrorEnum::NoOwnedBucket { .. }
        | QuadErrorEnum::SkolemDraws { .. }
        | QuadErrorEnum::UnknownSkolem { .. } => e.into(),
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
