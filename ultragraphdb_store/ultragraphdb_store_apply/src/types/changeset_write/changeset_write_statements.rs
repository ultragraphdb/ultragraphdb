/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphNameRef, NamedNode, NamedNodeRef, NamedOrBlankNodeRef, Term, TermRef};
use ultragraphdb_core_mapping::{MappingError, StatementRef};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_store_quads::StatementRecord;
use ultragraphdb_traits_store::{ListCache, Store};

use crate::errors::changeset_error::ChangesetError;
use crate::types::changeset_write::ChangesetWrite;
use crate::types::changeset_write::changeset_write_triples::Resolved;

impl<S, C> ChangesetWrite<'_, S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    /// Inserts a statement of its own: beside a held triple, which then gives every statement holding it an ID, or
    /// with properties, which give it one; otherwise as [`ChangesetWrite::insert`] writes it.
    pub(crate) fn insert_edge(
        &mut self,
        graph: GraphNameRef<'_>,
        subject: NamedOrBlankNodeRef<'_>,
        predicate: NamedNodeRef<'_>,
        object: TermRef<'_>,
        properties: &[(NamedNode, Term)],
    ) -> Result<(), ChangesetError> {
        let r = self.resolve(graph, subject, predicate, object)?;
        let declaration = self.checked(&r, predicate, object)?;
        let properties = properties
            .iter()
            .map(|(p, v)| Ok((self.iri(p.as_ref())?, self.term(p.as_ref(), v.as_ref())?)))
            .collect::<Result<Vec<_>, ChangesetError>>()?;
        match self.held(&r, &declaration)? {
            Some(mut ids) => {
                if ids.is_empty() {
                    ids.push(self.new_statement(&r, Vec::new())?);
                }
                ids.push(self.new_statement(&r, properties)?);
                self.write
                    .set_statement_ids(r.graph, r.subject, r.predicate, &r.object, &ids);
                Ok(())
            }
            None if properties.is_empty() => self.add(&r, &declaration, object, &[]),
            None => {
                let id = self.new_statement(&r, properties)?;
                self.add(&r, &declaration, object, &[id])
            }
        }
    }

    /// A new statement ID, its record holding the statement and `properties`.
    fn new_statement(
        &mut self,
        r: &Resolved,
        properties: Vec<(u64, EncodedTerm)>,
    ) -> Result<u64, ChangesetError> {
        let id = self.write.next_statement_id()?;
        let record = StatementRecord::new(r.subject, r.predicate, r.object.clone(), properties);
        self.records.insert((r.graph, id), Some(record));
        Ok(id)
    }

    /// Deletes statement `id`: its record, and its triple when no other statement holds it. An ID no statement has
    /// changes nothing.
    pub(crate) fn delete_statement(
        &mut self,
        graph: GraphNameRef<'_>,
        id: u64,
    ) -> Result<(), ChangesetError> {
        let Some(graph) = self.find_graph(graph)? else {
            return Ok(());
        };
        let Some(record) = self.record(graph, id)? else {
            return Ok(());
        };
        let (subject, predicate, object) = (
            record.subject(),
            record.predicate(),
            record.object().clone(),
        );
        let held = self.write.objects(graph, subject, predicate)?;
        let ids = held
            .into_iter()
            .find(|(o, _)| *o == object)
            .map(|(_, ids)| ids)
            .unwrap_or_default();
        let remaining: Vec<u64> = ids.into_iter().filter(|i| *i != id).collect();
        self.records.insert((graph, id), None);
        if remaining.is_empty() {
            let iri = self.decode(&EncodedTerm::uid(predicate))?;
            let declaration = match &iri {
                Term::NamedNode(n) => self.behaviour(predicate, n.as_ref()),
                _ => return Ok(()),
            };
            self.remove(graph, subject, predicate, &declaration, &object)?;
        } else {
            self.write
                .set_statement_ids(graph, subject, predicate, &object, &remaining);
        }
        Ok(())
    }

    /// Adds a property to a statement, giving it an ID when it has none.
    pub(crate) fn set_property(
        &mut self,
        statement: &StatementRef,
        predicate: NamedNodeRef<'_>,
        value: TermRef<'_>,
    ) -> Result<(), ChangesetError> {
        let (graph, id, mut record) = match self.statement(statement)? {
            (graph, Some(id), record) => (graph, id, record),
            (graph, None, record) => {
                let id = self.write.next_statement_id()?;
                self.write.set_statement_ids(
                    graph,
                    record.subject(),
                    record.predicate(),
                    record.object(),
                    &[id],
                );
                (graph, id, record)
            }
        };
        let property = (self.iri(predicate)?, self.term(predicate, value)?);
        if !record.properties().contains(&property) {
            record.properties_mut().push(property);
        }
        self.records.insert((graph, id), Some(record));
        Ok(())
    }

    /// Removes one value of a statement's property or, with no value, every value of it.
    pub(crate) fn delete_property(
        &mut self,
        statement: &StatementRef,
        predicate: NamedNodeRef<'_>,
        value: Option<TermRef<'_>>,
    ) -> Result<(), ChangesetError> {
        let (graph, Some(id), mut record) = self.statement(statement)? else {
            return Ok(());
        };
        let Some(uid) = self.find_iri(predicate)? else {
            return Ok(());
        };
        let value = match value {
            Some(v) => match self.find_term(predicate, v)? {
                Some(value) => Some(value),
                None => return Ok(()),
            },
            None => None,
        };
        record
            .properties_mut()
            .retain(|(p, v)| *p != uid || value.as_ref().is_some_and(|value| v != value));
        self.records.insert((graph, id), Some(record));
        Ok(())
    }

    /// The one statement `statement` names: its graph, its ID when it has one, and its record, empty of properties
    /// for a statement with no ID. Refused when no statement, or several, hold the triple named.
    fn statement(
        &mut self,
        statement: &StatementRef,
    ) -> Result<(u64, Option<u64>, StatementRecord), ChangesetError> {
        match statement {
            StatementRef::Id { graph, id } => {
                let missing =
                    || MappingError::NoSuchStatement(format!("{} in {graph}", id.value()));
                let uid = self.find_graph(graph.as_ref())?.ok_or_else(missing)?;
                let record = self.record(uid, id.value())?.ok_or_else(missing)?;
                Ok((uid, Some(id.value()), record))
            }
            StatementRef::Triple {
                graph,
                subject,
                predicate,
                object,
            } => {
                let text = match graph {
                    oxrdf::GraphName::DefaultGraph => format!("{subject} {predicate} {object}"),
                    named => format!("{subject} {predicate} {object} {named}"),
                };
                let r = self
                    .find(
                        graph.as_ref(),
                        subject.as_ref(),
                        predicate.as_ref(),
                        object.as_ref(),
                    )?
                    .ok_or_else(|| MappingError::NoSuchStatement(text.clone()))?;
                let held = self.write.objects(r.graph, r.subject, r.predicate)?;
                let ids = held
                    .into_iter()
                    .find(|(o, _)| *o == r.object)
                    .map(|(_, ids)| ids)
                    .ok_or_else(|| MappingError::NoSuchStatement(text.clone()))?;
                match ids.as_slice() {
                    [] => Ok((
                        r.graph,
                        None,
                        StatementRecord::new(r.subject, r.predicate, r.object, Vec::new()),
                    )),
                    [id] => {
                        let record = self
                            .record(r.graph, *id)?
                            .ok_or_else(|| MappingError::NoSuchStatement(text.clone()))?;
                        Ok((r.graph, Some(*id), record))
                    }
                    _ => Err(MappingError::AmbiguousStatement(text).into()),
                }
            }
        }
    }
}
