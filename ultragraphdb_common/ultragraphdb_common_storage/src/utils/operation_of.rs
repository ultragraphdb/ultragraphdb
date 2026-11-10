/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphName, NamedNode, NamedOrBlankNode, Term, TermRef};
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{
    Command, CommitOperation, ErrorCode, ErrorReply, Link, TermTableView,
};
use ultragraphdb_store_apply::{Operation, OperationLink};
use ultragraphdb_transactions_txn::FrameTerms;

/// The commit-time operation of an Operation frame. An operation is decided in the commit step, past its frame, so it
/// owns its terms.
///
/// # Errors
///
/// Code 1 naming the [`ProtocolError::field_value`] of a term of the wrong kind for its place; code 7, `detail` 0, for
/// kinds 5 and 6, which `PROTOCOL.md` §7.3 refuses in a tenant with no schema, as every tenant is until Install is
/// served.
pub(crate) fn operation_of(
    operation: &CommitOperation,
    table: &TermTableView<'_>,
    terms: &FrameTerms,
) -> Result<Operation, ErrorReply> {
    let instance = match operation {
        CommitOperation::DeleteInstance { .. } => Some((5, "delete an instance")),
        CommitOperation::RetractInstance { .. } => Some((6, "retract an instance")),
        CommitOperation::InsertIfAbsent { .. }
        | CommitOperation::UpsertAndLink { .. }
        | CommitOperation::CompareAndSet { .. }
        | CommitOperation::CounterAdd { .. } => None,
    };
    if let Some((kind, name)) = instance {
        return Err(ErrorReply::new(
            ErrorCode::Refused,
            format!("an Operation of kind {kind}, {name}, is refused: the tenant holds no schema"),
        ));
    }
    read_operation(operation, table, terms).map_err(|rule| ErrorReply::protocol(&rule))
}

/// The operation of kinds 1 to 4 `operation` names, its terms read from `table`.
fn read_operation(
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
        // `operation_of` refuses kinds 5 and 6 before reading.
        CommitOperation::DeleteInstance { .. } => {
            return Err(ProtocolError::field_value(
                Command::Operation.number(),
                "kind",
                5,
            ));
        }
        CommitOperation::RetractInstance { .. } => {
            return Err(ProtocolError::field_value(
                Command::Operation.number(),
                "kind",
                6,
            ));
        }
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
