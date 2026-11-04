/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphName, NamedNode, NamedOrBlankNode, Term, TermRef};
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{Command, CommitOperation, Link, TermTableView};
use ultragraphdb_store_apply::{Operation, OperationLink};
use ultragraphdb_transactions_txn::FrameTerms;

/// The commit-time operation of an Operation frame. An operation is decided in the commit step, past its frame, so it
/// owns its terms.
///
/// # Errors
///
/// [`ProtocolError::field_value`] naming a term of the wrong kind for its place.
pub(crate) fn operation_of(
    operation: &CommitOperation,
    table: &TermTableView<'_>,
    terms: &FrameTerms,
) -> Result<Operation, ProtocolError> {
    let reader = Reader { table, terms };
    Ok(match operation {
        CommitOperation::InsertIfAbsent {
            graph,
            predicate,
            value,
            properties,
        } => Operation::InsertIfAbsent {
            graph: reader.graph(*graph)?,
            predicate: reader.iri(*predicate, "predicate")?,
            value: reader.term(*value)?,
            properties: reader.properties(properties)?,
        },
        CommitOperation::UpsertAndLink {
            graph,
            predicate,
            value,
            properties,
            links,
        } => Operation::UpsertAndLink {
            graph: reader.graph(*graph)?,
            predicate: reader.iri(*predicate, "predicate")?,
            value: reader.term(*value)?,
            properties: reader.properties(properties)?,
            links: links
                .iter()
                .map(|link| reader.link(link))
                .collect::<Result<Vec<_>, _>>()?,
        },
        CommitOperation::CompareAndSet {
            graph,
            subject,
            predicate,
            expected,
            new,
        } => Operation::CompareAndSet {
            graph: reader.graph(*graph)?,
            subject: reader.node(*subject, "subject")?,
            predicate: reader.iri(*predicate, "predicate")?,
            expected: reader.term(*expected)?,
            new: reader.term(*new)?,
        },
        CommitOperation::CounterAdd {
            graph,
            subject,
            predicate,
            delta,
        } => Operation::CounterAdd {
            graph: reader.graph(*graph)?,
            subject: reader.node(*subject, "subject")?,
            predicate: reader.iri(*predicate, "predicate")?,
            delta: *delta,
        },
    })
}

struct Reader<'a, 'v> {
    table: &'v TermTableView<'a>,
    terms: &'v FrameTerms,
}

impl Reader<'_, '_> {
    fn term(&self, index: u32) -> Result<Term, ProtocolError> {
        Ok(self.terms.term(self.table, index)?.into_owned())
    }

    fn iri(&self, index: u32, place: &'static str) -> Result<NamedNode, ProtocolError> {
        match self.terms.term(self.table, index)? {
            TermRef::NamedNode(iri) => Ok(iri.into_owned()),
            TermRef::BlankNode(_) | TermRef::Literal(_) | TermRef::Triple(_) => {
                Err(refused(place, index))
            }
        }
    }

    fn node(&self, index: u32, place: &'static str) -> Result<NamedOrBlankNode, ProtocolError> {
        match self.terms.term(self.table, index)? {
            TermRef::NamedNode(iri) => Ok(NamedOrBlankNode::NamedNode(iri.into_owned())),
            TermRef::BlankNode(blank) => Ok(NamedOrBlankNode::BlankNode(blank.into_owned())),
            TermRef::Literal(_) | TermRef::Triple(_) => Err(refused(place, index)),
        }
    }

    fn graph(&self, index: Option<u32>) -> Result<GraphName, ProtocolError> {
        let Some(index) = index else {
            return Ok(GraphName::DefaultGraph);
        };
        Ok(match self.node(index, "graph")? {
            NamedOrBlankNode::NamedNode(iri) => GraphName::NamedNode(iri),
            NamedOrBlankNode::BlankNode(blank) => GraphName::BlankNode(blank),
        })
    }

    fn properties(
        &self,
        properties: &[(u32, u32)],
    ) -> Result<Vec<(NamedNode, Term)>, ProtocolError> {
        properties
            .iter()
            .map(|(key, value)| Ok((self.iri(*key, "property")?, self.term(*value)?)))
            .collect()
    }

    fn link(&self, link: &Link) -> Result<OperationLink, ProtocolError> {
        let predicate = self.iri(link.predicate(), "link")?;
        let other = self.node(link.other(), "link")?;
        Ok(if link.node_is_subject() {
            OperationLink::from_node(predicate, other)
        } else {
            OperationLink::to_node(predicate, other)
        })
    }
}

fn refused(place: &'static str, index: u32) -> ProtocolError {
    ProtocolError::field_value(Command::Operation.number(), place, index.into())
}
