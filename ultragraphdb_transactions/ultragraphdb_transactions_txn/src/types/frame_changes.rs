/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

use oxilangtag::LanguageTag;
use oxrdf::{
    BaseDirection, BlankNodeRef, GraphNameRef, Literal, LiteralRef, NamedNode, NamedNodeRef,
    NamedOrBlankNode, NamedOrBlankNodeRef, Term, TermRef, Triple,
};
use ultragraphdb_core_mapping::{ChangeRef, Changes, StatementView};
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{
    Command, Direction, RequestTermView, StatementTarget, TermTableView, WriteChange, WriteView,
};

/// The terms of a Write's table whose RDF form its text cannot lend: a triple term, which RDF holds as a value, and a
/// language tag not in the lowercase RDF holds. Every other term is validated here and borrowed from the frame.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrameTerms {
    owned: BTreeMap<u32, Term>,
}

impl FrameTerms {
    /// Validates every term of `table`, as the RDF model takes it.
    ///
    /// # Errors
    ///
    /// [`ProtocolError::field_value`] naming the entry that is no IRI, blank node label, literal or triple term.
    pub fn of(table: &TermTableView<'_>) -> Result<Self, ProtocolError> {
        let mut terms = Self::default();
        let entries = u32::try_from(table.len()).unwrap_or(u32::MAX);
        for index in 0..entries {
            let entry = table.get(index).ok_or_else(|| refused("term", index))?;
            if let Some(owned) = terms.validated(table, index, entry)? {
                let _none = terms.owned.insert(index, owned);
            }
        }
        Ok(terms)
    }

    /// Checks entry `index`; its owned form, when its text cannot lend one.
    fn validated(
        &self,
        table: &TermTableView<'_>,
        index: u32,
        entry: RequestTermView<'_>,
    ) -> Result<Option<Term>, ProtocolError> {
        let wrong = || refused("term", index);
        match entry {
            RequestTermView::Iri(iri) => {
                let _valid = NamedNodeRef::new(iri).map_err(|_| wrong())?;
                Ok(None)
            }
            RequestTermView::BlankNode(label) => {
                let _valid = BlankNodeRef::new(label).map_err(|_| wrong())?;
                Ok(None)
            }
            RequestTermView::Literal { datatype, .. } => {
                if let Some(datatype) = datatype {
                    let _valid = NamedNodeRef::new(datatype).map_err(|_| wrong())?;
                }
                Ok(None)
            }
            RequestTermView::LanguageLiteral {
                lexical,
                language,
                direction,
            } => {
                let _valid = LanguageTag::parse(language).map_err(|_| wrong())?;
                if language.bytes().any(|byte| byte.is_ascii_uppercase()) {
                    let lowered = language.to_ascii_lowercase();
                    return Ok(Some(Term::Literal(match direction {
                        Some(direction) => {
                            Literal::new_directional_language_tagged_literal_unchecked(
                                lexical,
                                lowered,
                                base_direction(direction),
                            )
                        }
                        None => Literal::new_language_tagged_literal_unchecked(lexical, lowered),
                    })));
                }
                Ok(None)
            }
            RequestTermView::Triple {
                subject,
                predicate,
                object,
            } => {
                let subject = match self.term(table, subject)? {
                    TermRef::NamedNode(iri) => NamedOrBlankNode::NamedNode(iri.into_owned()),
                    TermRef::BlankNode(blank) => NamedOrBlankNode::BlankNode(blank.into_owned()),
                    TermRef::Literal(_) | TermRef::Triple(_) => return Err(wrong()),
                };
                let TermRef::NamedNode(predicate) = self.term(table, predicate)? else {
                    return Err(wrong());
                };
                let predicate: NamedNode = predicate.into_owned();
                let object = self.term(table, object)?.into_owned();
                Ok(Some(Term::Triple(Box::new(Triple::new(
                    subject, predicate, object,
                )))))
            }
        }
    }

    /// Entry `index` as an RDF term, borrowed from the frame or from what this holds.
    ///
    /// # Errors
    ///
    /// [`ProtocolError::field_value`] for an index past the table.
    pub fn term<'a>(
        &'a self,
        table: &TermTableView<'a>,
        index: u32,
    ) -> Result<TermRef<'a>, ProtocolError> {
        if let Some(owned) = self.owned.get(&index) {
            return Ok(owned.as_ref());
        }
        let entry = table.get(index).ok_or_else(|| refused("term", index))?;
        Ok(match entry {
            RequestTermView::Iri(iri) => TermRef::NamedNode(NamedNodeRef::new_unchecked(iri)),
            RequestTermView::BlankNode(label) => {
                TermRef::BlankNode(BlankNodeRef::new_unchecked(label))
            }
            RequestTermView::Literal {
                lexical,
                datatype: None,
            } => TermRef::Literal(LiteralRef::new_simple_literal(lexical)),
            RequestTermView::Literal {
                lexical,
                datatype: Some(datatype),
            } => TermRef::Literal(LiteralRef::new_typed_literal(
                lexical,
                NamedNodeRef::new_unchecked(datatype),
            )),
            RequestTermView::LanguageLiteral {
                lexical,
                language,
                direction: None,
            } => TermRef::Literal(LiteralRef::new_language_tagged_literal_unchecked(
                lexical, language,
            )),
            RequestTermView::LanguageLiteral {
                lexical,
                language,
                direction: Some(direction),
            } => TermRef::Literal(
                LiteralRef::new_directional_language_tagged_literal_unchecked(
                    lexical,
                    language,
                    base_direction(direction),
                ),
            ),
            // A triple term is always held, once `FrameTerms::of` has passed it.
            RequestTermView::Triple { .. } => return Err(refused("term", index)),
        })
    }
}

/// The changes a Write frame carries, in the frame's own bytes: each term borrowed from the frame until the write
/// resolves it to its ID, so no term is copied per change. The one path [`crate::Transaction::write`] takes for a
/// [`ultragraphdb_core_mapping::Changeset`] takes these too, so the two give one record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameChanges<'a> {
    tenant: u64,
    changes: Vec<ChangeRef<'a>>,
}

impl<'a> FrameChanges<'a> {
    /// The changes of `write`, in the transaction's `tenant`, over `terms`, what [`FrameTerms::of`] made of its table.
    ///
    /// # Errors
    ///
    /// [`ProtocolError::field_value`] naming a term of the wrong kind for its place, or a declaration, which commits as
    /// a catalog version of the metadata group and never reaches a partition (`PROTOCOL.md` §7.2).
    pub fn new(
        tenant: u64,
        write: &WriteView<'a>,
        terms: &'a FrameTerms,
    ) -> Result<Self, ProtocolError> {
        let reader = Reader {
            table: write.terms(),
            terms,
        };
        let changes = (0..write.len())
            .map(|index| {
                let change = write
                    .change(index)
                    .ok_or_else(|| refused("change", index as u32))?;
                reader.change(change)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { tenant, changes })
    }
}

impl Changes for FrameChanges<'_> {
    fn tenant(&self) -> u64 {
        self.tenant
    }

    fn changes(&self) -> impl Iterator<Item = ChangeRef<'_>> {
        self.changes.iter().cloned()
    }
}

/// Reads a Write's records against its table.
struct Reader<'a, 'v> {
    table: &'v TermTableView<'a>,
    terms: &'a FrameTerms,
}

impl<'a> Reader<'a, '_> {
    fn term(&self, index: u32) -> Result<TermRef<'a>, ProtocolError> {
        self.terms.term(self.table, index)
    }

    fn iri(&self, index: u32, place: &'static str) -> Result<NamedNodeRef<'a>, ProtocolError> {
        match self.term(index)? {
            TermRef::NamedNode(iri) => Ok(iri),
            TermRef::BlankNode(_) | TermRef::Literal(_) | TermRef::Triple(_) => {
                Err(refused(place, index))
            }
        }
    }

    fn node(
        &self,
        index: u32,
        place: &'static str,
    ) -> Result<NamedOrBlankNodeRef<'a>, ProtocolError> {
        match self.term(index)? {
            TermRef::NamedNode(iri) => Ok(NamedOrBlankNodeRef::NamedNode(iri)),
            TermRef::BlankNode(blank) => Ok(NamedOrBlankNodeRef::BlankNode(blank)),
            TermRef::Literal(_) | TermRef::Triple(_) => Err(refused(place, index)),
        }
    }

    fn graph(&self, index: Option<u32>) -> Result<GraphNameRef<'a>, ProtocolError> {
        let Some(index) = index else {
            return Ok(GraphNameRef::DefaultGraph);
        };
        Ok(match self.node(index, "graph")? {
            NamedOrBlankNodeRef::NamedNode(iri) => GraphNameRef::NamedNode(iri),
            NamedOrBlankNodeRef::BlankNode(blank) => GraphNameRef::BlankNode(blank),
        })
    }

    fn statement(&self, target: StatementTarget) -> Result<StatementView<'a>, ProtocolError> {
        Ok(match target {
            StatementTarget::Reifier { graph, reifier } => StatementView::Reifier {
                graph: self.graph(graph)?,
                reifier: self.iri(reifier, "reifier")?,
            },
            StatementTarget::Triple {
                graph,
                subject,
                predicate,
                object,
            } => StatementView::Triple {
                graph: self.graph(graph)?,
                subject: self.node(subject, "subject")?,
                predicate: self.iri(predicate, "predicate")?,
                object: self.term(object)?,
            },
        })
    }

    fn properties(
        &self,
        properties: Vec<(u32, u32)>,
    ) -> Result<Vec<(NamedNodeRef<'a>, TermRef<'a>)>, ProtocolError> {
        properties
            .into_iter()
            .map(|(key, value)| Ok((self.iri(key, "property")?, self.term(value)?)))
            .collect()
    }

    fn change(&self, change: WriteChange) -> Result<ChangeRef<'a>, ProtocolError> {
        Ok(match change {
            WriteChange::Insert {
                graph,
                subject,
                predicate,
                object,
            } => ChangeRef::Insert {
                graph: self.graph(graph)?,
                subject: self.node(subject, "subject")?,
                predicate: self.iri(predicate, "predicate")?,
                object: self.term(object)?,
            },
            WriteChange::InsertEdge {
                graph,
                subject,
                predicate,
                object,
                properties,
            } => ChangeRef::InsertEdge {
                graph: self.graph(graph)?,
                subject: self.node(subject, "subject")?,
                predicate: self.iri(predicate, "predicate")?,
                object: self.term(object)?,
                properties: self.properties(properties)?,
            },
            WriteChange::Delete {
                graph,
                subject,
                predicate,
                object,
            } => ChangeRef::Delete {
                graph: self.graph(graph)?,
                subject: self.node(subject, "subject")?,
                predicate: self.iri(predicate, "predicate")?,
                object: self.term(object)?,
            },
            WriteChange::DeleteStatement { graph, reifier } => ChangeRef::DeleteStatement {
                graph: self.graph(graph)?,
                reifier: self.iri(reifier, "reifier")?,
            },
            WriteChange::DeletePattern {
                graph,
                subject,
                predicate,
            } => ChangeRef::DeletePattern {
                graph: self.graph(graph)?,
                subject: self.node(subject, "subject")?,
                predicate: self.iri(predicate, "predicate")?,
            },
            WriteChange::SetProperty {
                statement,
                predicate,
                value,
            } => ChangeRef::SetProperty {
                statement: self.statement(statement)?,
                predicate: self.iri(predicate, "predicate")?,
                value: self.term(value)?,
            },
            WriteChange::DeleteProperty {
                statement,
                predicate,
                value,
            } => ChangeRef::DeleteProperty {
                statement: self.statement(statement)?,
                predicate: self.iri(predicate, "predicate")?,
                value: value.map(|value| self.term(value)).transpose()?,
            },
            WriteChange::Declare(_) => return Err(refused("declaration", 0)),
        })
    }
}

fn base_direction(direction: Direction) -> BaseDirection {
    match direction {
        Direction::Ltr => BaseDirection::Ltr,
        Direction::Rtl => BaseDirection::Rtl,
    }
}

fn refused(place: &'static str, index: u32) -> ProtocolError {
    ProtocolError::field_value(Command::Write.number(), place, index.into())
}
