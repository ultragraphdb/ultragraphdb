/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphName, NamedNode, NamedOrBlankNode, Term};

use crate::errors::mapping_error::MappingError;
use crate::types::canonical_dataset::{CanonicalDataset, Held, Record, key};
use crate::types::cardinality::Cardinality;
use crate::types::change::Change;
use crate::types::changeset::Changeset;
use crate::types::statement_id::StatementId;
use crate::types::statement_ref::StatementRef;

impl CanonicalDataset {
    /// Applies `changeset`'s changes in order, whole or not at all: on a refusal nothing of it is held.
    /// `cardinality` says which predicates are single-valued; declarations are the catalog's and change nothing here.
    pub fn apply(
        &mut self,
        changeset: &Changeset,
        cardinality: impl Fn(&NamedNode) -> Cardinality,
    ) -> Result<(), MappingError> {
        let mut next = self.clone();
        for change in changeset.changes() {
            next.change(change, &cardinality)?;
        }
        *self = next;
        Ok(())
    }

    fn change(
        &mut self,
        change: &Change,
        cardinality: &impl Fn(&NamedNode) -> Cardinality,
    ) -> Result<(), MappingError> {
        match change {
            Change::Insert {
                graph,
                subject,
                predicate,
                object,
            } => {
                refuse_triple_term(object)?;
                self.replace_single(graph, subject, predicate, object, cardinality);
                let held = self.held(graph, subject, predicate, object);
                if held.records.is_empty() {
                    held.records.push(Record {
                        id: None,
                        properties: Vec::new(),
                    });
                }
            }
            Change::InsertEdge {
                graph,
                subject,
                predicate,
                object,
                properties,
            } => {
                refuse_triple_term(object)?;
                self.replace_single(graph, subject, predicate, object, cardinality);
                let mut next_id = self.next_id;
                let mut allocate = || {
                    next_id += 1;
                    StatementId::new(next_id)
                };
                let held = self.held(graph, subject, predicate, object);
                let repeats = !held.records.is_empty();
                if repeats {
                    held.records
                        .iter_mut()
                        .filter(|r| r.id.is_none())
                        .for_each(|r| r.id = Some(allocate()));
                }
                let id = (repeats || !properties.is_empty()).then(&mut allocate);
                held.records.push(Record {
                    id,
                    properties: properties.clone(),
                });
                self.next_id = next_id;
            }
            Change::Delete {
                graph,
                subject,
                predicate,
                object,
            } => {
                self.triples.remove(&key(graph, subject, predicate, object));
            }
            Change::DeleteStatement { graph, id } => {
                self.triples.retain(|_, held| {
                    if held.graph == *graph {
                        held.records.retain(|r| r.id != Some(*id));
                    }
                    !held.records.is_empty()
                });
            }
            Change::DeletePattern {
                graph,
                subject,
                predicate,
            } => self.delete_pattern(graph, subject, predicate),
            Change::SetProperty {
                statement,
                predicate,
                value,
            } => {
                let mut next_id = self.next_id;
                let record = self.record(statement)?;
                if record.id.is_none() {
                    next_id += 1;
                    record.id = Some(StatementId::new(next_id));
                }
                let property = (predicate.clone(), value.clone());
                if !record.properties.contains(&property) {
                    record.properties.push(property);
                }
                self.next_id = next_id;
            }
            Change::DeleteProperty {
                statement,
                predicate,
                value,
            } => {
                let record = self.record(statement)?;
                record.properties.retain(|(p, v)| {
                    p != predicate || value.as_ref().is_some_and(|value| v != value)
                });
            }
            Change::Declare(_) => {}
        }
        Ok(())
    }

    /// Removes the subject's other objects of a single-valued predicate, as a write of `object` replaces them.
    fn replace_single(
        &mut self,
        graph: &GraphName,
        subject: &NamedOrBlankNode,
        predicate: &NamedNode,
        object: &Term,
        cardinality: &impl Fn(&NamedNode) -> Cardinality,
    ) {
        if cardinality(predicate) == Cardinality::Single {
            self.triples.retain(|_, h| {
                !(h.graph == *graph && h.subject == *subject && h.predicate == *predicate)
                    || h.object == *object
            });
        }
    }

    fn held(
        &mut self,
        graph: &GraphName,
        subject: &NamedOrBlankNode,
        predicate: &NamedNode,
        object: &Term,
    ) -> &mut Held {
        self.triples
            .entry(key(graph, subject, predicate, object))
            .or_insert_with(|| Held {
                graph: graph.clone(),
                subject: subject.clone(),
                predicate: predicate.clone(),
                object: object.clone(),
                records: Vec::new(),
            })
    }

    fn delete_pattern(
        &mut self,
        graph: &GraphName,
        subject: &NamedOrBlankNode,
        predicate: &NamedNode,
    ) {
        self.triples.retain(|_, h| {
            !(h.graph == *graph && h.subject == *subject && h.predicate == *predicate)
        });
    }

    /// The one statement `statement` names.
    fn record(&mut self, statement: &StatementRef) -> Result<&mut Record, MappingError> {
        match statement {
            StatementRef::Id { graph, id } => self
                .triples
                .values_mut()
                .filter(|h| h.graph == *graph)
                .flat_map(|h| h.records.iter_mut())
                .find(|r| r.id == Some(*id))
                .ok_or_else(|| MappingError::NoSuchStatement(format!("{} in {graph}", id.value()))),
            StatementRef::Triple {
                graph,
                subject,
                predicate,
                object,
            } => {
                let text = key(graph, subject, predicate, object);
                let held = self
                    .triples
                    .get_mut(&text)
                    .ok_or_else(|| MappingError::NoSuchStatement(text.clone()))?;
                match held.records.as_mut_slice() {
                    [one] => Ok(one),
                    _ => Err(MappingError::AmbiguousStatement(text)),
                }
            }
        }
    }
}

fn refuse_triple_term(object: &Term) -> Result<(), MappingError> {
    match object {
        Term::Triple(t) => Err(MappingError::UnsupportedObject(t.to_string())),
        _ => Ok(()),
    }
}
