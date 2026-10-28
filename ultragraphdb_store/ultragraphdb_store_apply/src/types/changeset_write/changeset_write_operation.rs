/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Commit-time operations on a changeset's write (7.5): each resolves its node or reads its value through the write's
//! request, which reads the store at the watermark with the decided but unapplied rounds' writes stacked above it, and
//! writes through the same index maintenance as a changeset's changes.

use oxrdf::vocab::xsd;
use oxrdf::{
    BlankNode, GraphNameRef, Literal, NamedNode, NamedNodeRef, NamedOrBlankNodeRef, Term, TermRef,
};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_store::{ListCache, Store};

use crate::errors::changeset_error::ChangesetError;
use crate::types::changeset_write::ChangesetWrite;
use crate::types::changeset_write::changeset_write_triples::Resolved;
use crate::types::operation::Operation;
use crate::types::operation_link::OperationLink;

impl<S, C> ChangesetWrite<'_, S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    /// Runs `operation`, the `index`th of its transaction: the UID of the node an insert-if-absent or upsert-and-link
    /// found or created and whether it created it, zero and `false` for a compare-and-set and a counter add.
    pub(crate) fn operate(
        &mut self,
        operation: &Operation,
        index: usize,
    ) -> Result<(u64, bool), ChangesetError> {
        match operation {
            Operation::InsertIfAbsent {
                graph,
                predicate,
                value,
                properties,
            } => self.upsert(
                graph.as_ref(),
                predicate.as_ref(),
                value.as_ref(),
                properties,
                &[],
                index,
            ),
            Operation::UpsertAndLink {
                graph,
                predicate,
                value,
                properties,
                links,
            } => self.upsert(
                graph.as_ref(),
                predicate.as_ref(),
                value.as_ref(),
                properties,
                links,
                index,
            ),
            Operation::CompareAndSet {
                graph,
                subject,
                predicate,
                expected,
                new,
            } => self
                .compare_and_set(
                    graph.as_ref(),
                    subject.as_ref(),
                    predicate.as_ref(),
                    expected.as_ref(),
                    new.as_ref(),
                )
                .map(|()| (0, false)),
            Operation::CounterAdd {
                graph,
                subject,
                predicate,
                delta,
            } => self
                .counter_add(graph.as_ref(), subject.as_ref(), predicate.as_ref(), *delta)
                .map(|()| (0, false)),
        }
    }

    /// The node holding `value` of the unique `predicate`, the smallest UID when several do, else a new blank node
    /// holding it with `properties`; then `links`, each an edge between the node and another node.
    fn upsert(
        &mut self,
        graph: GraphNameRef<'_>,
        predicate: NamedNodeRef<'_>,
        value: TermRef<'_>,
        properties: &[(NamedNode, Term)],
        links: &[OperationLink],
        index: usize,
    ) -> Result<(u64, bool), ChangesetError> {
        let g = self.graph(graph)?;
        let p = self.iri(predicate)?;
        let encoded = self.term(predicate, value)?;
        if !self.declaration(p, predicate)?.unique() {
            return Err(ChangesetError::NotUnique(predicate.as_str()));
        }
        let held = self.write.subjects(g, p, &encoded)?.into_iter().min();
        let (node, created) = match held {
            Some(node) => (node, false),
            None => {
                // A label no changeset can name: a blank node's label holds no space.
                let blank = BlankNode::new_unchecked(format!(" operation {index}"));
                let subject = NamedOrBlankNodeRef::BlankNode(blank.as_ref());
                self.insert(graph, subject, predicate, value)?;
                for (property, v) in properties {
                    self.insert(graph, subject, property.as_ref(), v.as_ref())?;
                }
                (self.node(subject)?, true)
            }
        };
        let as_object = self.node_term(node)?;
        for link in links {
            let lp = self.iri(link.predicate().as_ref())?;
            let other = self.node(link.other().as_ref())?;
            let (r, object) = if link.node_is_subject() {
                let resolved = Resolved {
                    graph: g,
                    subject: node,
                    predicate: lp,
                    object: EncodedTerm::uid(other),
                };
                (resolved, Term::from(link.other().clone()))
            } else {
                let resolved = Resolved {
                    graph: g,
                    subject: other,
                    predicate: lp,
                    object: EncodedTerm::uid(node),
                };
                (resolved, as_object.clone())
            };
            self.write_resolved(&r, link.predicate().as_ref(), object.as_ref())?;
        }
        Ok((node, created))
    }

    /// The node `uid` as a term, for its kind: the write's own IRI or skolem, the store's, or a blank node when its
    /// dictionary entry is in a round not yet applied, which only an operation's or a write's creation is.
    fn node_term(&mut self, uid: u64) -> Result<Term, ChangesetError> {
        match self.decode(&EncodedTerm::uid(uid)) {
            Ok(term) => Ok(term),
            Err(_) => Ok(BlankNode::new_unchecked(format!("b{uid}")).into()),
        }
    }

    /// Writes the resolved statement as an insert does: checked against the catalog, held once.
    fn write_resolved(
        &mut self,
        r: &Resolved,
        predicate: NamedNodeRef<'_>,
        object: TermRef<'_>,
    ) -> Result<(), ChangesetError> {
        let declaration = self.checked(r, predicate, object)?;
        if self.held(r, &declaration)?.is_some() {
            return Ok(());
        }
        self.add(r, &declaration, object, &[])
    }

    /// `subject`'s value of `predicate` replaced by `new` when it is `expected` and nothing else.
    fn compare_and_set(
        &mut self,
        graph: GraphNameRef<'_>,
        subject: NamedOrBlankNodeRef<'_>,
        predicate: NamedNodeRef<'_>,
        expected: TermRef<'_>,
        new: TermRef<'_>,
    ) -> Result<(), ChangesetError> {
        let (g, s, p) = (
            self.graph(graph)?,
            self.node(subject)?,
            self.iri(predicate)?,
        );
        let held = self.write.objects(g, s, p)?;
        let expected = self.find_term(predicate, expected)?;
        match held.as_slice() {
            [(object, _)] if Some(object) == expected.as_ref() => {
                let declaration = self.behaviour(p, predicate);
                self.remove(g, s, p, &declaration, object)?;
                self.insert(graph, subject, predicate, new)
            }
            [] => Err(ChangesetError::OperationFailed(None)),
            [(object, _), ..] => Err(ChangesetError::OperationFailed(Some(
                self.decode(object)?.to_string(),
            ))),
        }
    }

    /// `delta` added to `subject`'s integer value of `predicate`, 0 when it holds none.
    fn counter_add(
        &mut self,
        graph: GraphNameRef<'_>,
        subject: NamedOrBlankNodeRef<'_>,
        predicate: NamedNodeRef<'_>,
        delta: i64,
    ) -> Result<(), ChangesetError> {
        let (g, s, p) = (
            self.graph(graph)?,
            self.node(subject)?,
            self.iri(predicate)?,
        );
        let held = self.write.objects(g, s, p)?;
        let current = match held.as_slice() {
            [] => None,
            [(object, _)] => Some((object.clone(), self.decode(object)?)),
            [(object, _), ..] => {
                return Err(ChangesetError::OperationFailed(Some(
                    self.decode(object)?.to_string(),
                )));
            }
        };
        let value = match &current {
            None => 0,
            Some((_, term)) => integer(term)
                .ok_or_else(|| ChangesetError::OperationFailed(Some(term.to_string())))?,
        };
        let sum = value.checked_add(delta).ok_or_else(|| {
            ChangesetError::OperationFailed(current.as_ref().map(|(_, t)| t.to_string()))
        })?;
        if let Some((object, _)) = &current {
            let declaration = self.behaviour(p, predicate);
            self.remove(g, s, p, &declaration, object)?;
        }
        let sum = Literal::from(sum);
        self.insert(graph, subject, predicate, sum.as_ref().into())
    }
}

/// The value of an `xsd:integer` literal within `i64`.
fn integer(term: &Term) -> Option<i64> {
    match term {
        Term::Literal(l) if l.datatype() == xsd::INTEGER => l.value().parse().ok(),
        _ => None,
    }
}
