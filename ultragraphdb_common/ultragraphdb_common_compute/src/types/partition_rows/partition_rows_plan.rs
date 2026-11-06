/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::HashMap;

use ultragraphdb_core_ids::{SystemId, TypePrefix};
use ultragraphdb_core_keys::{Graph, key_prefix};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_wire::{ErrorReply, RequestTerm};

use super::{Graphs, PartitionRows, RowFilter, Skip};
use crate::types::access::{Access, RangeSpec};
use crate::types::read_request::{GraphSlot, ReadRequest};
use crate::types::resume_at::ResumeAt;
use crate::utils::literals::{datatype_named, encoding_of, is_vector};
use crate::utils::refusals::term_refused;

/// The IRIs a read of `read` resolves in a partition whose lists it reads: its graph's, subject's, predicate's, an IRI object's
/// unless its ID comes from its home, and the datatype a literal names by ID.
pub(crate) fn terms_of(read: &ReadRequest, object_from_home: bool) -> Vec<String> {
    let mut terms = Vec::with_capacity(4);
    if let GraphSlot::Named(graph) = &read.graph {
        terms.push(graph.clone());
    }
    terms.extend(read.subject.iter().cloned());
    terms.extend(read.predicate.iter().cloned());
    match &read.object {
        Some(RequestTerm::Iri(object)) if !object_from_home => terms.push(object.clone()),
        Some(object) => terms.extend(datatype_named(object).map(str::to_string)),
        None => {}
    }
    terms
}

/// What a partition's terms resolved a bound object to: its encoding, `None` when that partition holds no statement of it.
fn object_encoding(
    object: &RequestTerm,
    ids: &HashMap<String, SystemId>,
    home_object: Option<SystemId>,
) -> Result<Option<EncodedTerm>, ErrorReply> {
    if let RequestTerm::Iri(iri) = object {
        return Ok(home_object
            .or_else(|| ids.get(iri).copied())
            .map(EncodedTerm::id));
    }
    let datatype = datatype_named(object).and_then(|iri| ids.get(iri).copied());
    encoding_of(object, datatype).map_err(|error| term_refused(0, &error.to_string()))
}

impl PartitionRows {
    /// Sets how the partition reads the pattern, by the IDs `ids` its dictionary holds and `home_object`, the object's ID from its
    /// home: a bound term it lacks means it holds no statement of the term, and it answers no row.
    pub(crate) fn plan(
        &mut self,
        ids: &HashMap<String, SystemId>,
        home_object: Option<SystemId>,
    ) -> Result<(), ErrorReply> {
        let shared = std::rc::Rc::clone(&self.shared);
        let read = &shared.read;
        let object = match &read.object {
            Some(object) => match object_encoding(object, ids, home_object)? {
                Some(encoded) => Some(encoded),
                None => return self.no_rows(),
            },
            None => None,
        };
        let Some((access, filter)) = self.access_of(read, ids, object) else {
            return self.no_rows();
        };
        let graphs = match &read.graph {
            GraphSlot::Default => Graphs::One(Graph::Default),
            GraphSlot::Named(iri) => match ids.get(iri) {
                Some(id) => Graphs::One(Graph::Named(*id)),
                None => return self.no_rows(),
            },
            GraphSlot::Unbound => Graphs::Walk {
                keyspace: access.keyspace(),
                from: key_prefix(access.keyspace(), false, read.tenant, None, None),
            },
        };
        self.access = Some(access);
        self.filter = filter;
        self.graphs = graphs;
        Ok(())
    }

    fn no_rows(&mut self) -> Result<(), ErrorReply> {
        self.end();
        Ok(())
    }

    /// The partition answers no row.
    pub(crate) fn end(&mut self) {
        self.ended = true;
    }

    /// The access of the pattern, `None` when a bound term is absent. A predicate's absence ends it wherever it is bound.
    fn access_of(
        &self,
        read: &ReadRequest,
        ids: &HashMap<String, SystemId>,
        object: Option<EncodedTerm>,
    ) -> Option<(Access, RowFilter)> {
        let predicate = match &read.predicate {
            Some(iri) => Some(*ids.get(iri)?),
            None => None,
        };
        let vector = read.object.as_ref().is_some_and(is_vector);
        let bytes = object.as_ref().map(|term| term.as_bytes().to_vec());
        if let Some(subject) = &read.subject {
            let subject = *ids.get(subject)?;
            if subject.type_prefix() == TypePrefix::STATEMENT {
                let filter = RowFilter {
                    object: bytes,
                    property: predicate,
                };
                return Some((Access::Statement { statement: subject }, filter));
            }
            let filter = RowFilter {
                object: bytes,
                property: None,
            };
            let access = match predicate {
                Some(predicate) => Access::ForwardKey { predicate, subject },
                None => Access::SubjectPredicates { subject },
            };
            return Some((access, filter));
        }
        Some(match object {
            Some(_) if vector => (
                Access::ForwardRange { predicate },
                RowFilter {
                    object: bytes,
                    property: None,
                },
            ),
            Some(object) => (
                match predicate {
                    Some(predicate) => Access::ReverseKey { predicate, object },
                    None => Access::ObjectPredicates { object },
                },
                RowFilter::default(),
            ),
            None => (Access::ForwardRange { predicate }, RowFilter::default()),
        })
    }

    /// Continues the read after `at`, in the graph it names and the keys from there on.
    pub(crate) fn resume_at(&mut self, at: ResumeAt) {
        if let (Graphs::Walk { keyspace, from }, Some(access)) = (&mut self.graphs, &self.access) {
            *from = key_prefix(
                *keyspace,
                false,
                self.shared.read.tenant,
                Some(at.graph),
                None,
            );
            debug_assert_eq!(
                *keyspace,
                access.keyspace(),
                "a walk reads its access's keyspace"
            );
        }
        self.resume = Some(at);
    }

    /// The range of what the access reads in `graph`, from where a resumed read continues when it stopped in that graph.
    pub(super) fn range_in(&mut self, access: &Access, graph: Graph) -> RangeSpec {
        let tenant = self.shared.read.tenant;
        let range = access.range(tenant, graph);
        let Some(at) = self.resume.take_if(|at| at.graph == graph) else {
            return range;
        };
        match access {
            Access::ForwardKey { .. } | Access::ReverseKey { .. } => {
                self.skip_object(&range.first, access, &at);
                RangeSpec {
                    after: resumed_object(access, &at),
                    ..range
                }
            }
            Access::ForwardRange { .. } => {
                let first = ultragraphdb_core_keys::QuadKey::Forward {
                    tenant,
                    graph,
                    predicate: at.predicate,
                    subject: at.subject,
                }
                .to_bytes();
                self.skip_object(&first, access, &at);
                RangeSpec {
                    first,
                    past: range.past,
                    after: at.object.as_bytes().to_vec(),
                }
            }
            Access::SubjectPredicates { .. } | Access::ObjectPredicates { .. } => {
                let after = EncodedTerm::id(at.predicate).as_bytes().to_vec();
                self.resume_listed = Some(at);
                RangeSpec { after, ..range }
            }
            Access::Statement { .. } => {
                self.skip = Some(Skip::Property {
                    predicate: at.predicate,
                    value: at.object,
                });
                range
            }
        }
    }

    /// Drops the row of `at` from the list of `key`.
    pub(super) fn skip_object(&mut self, key: &[u8], access: &Access, at: &ResumeAt) {
        self.skip = Some(Skip::Object {
            key: key.to_vec(),
            encoding: resumed_object(access, at),
            statement: at.statement,
        });
    }
}

/// The object encoding a list of `access` holds the row of `at` under: a forward list's object, a reverse list's subject.
fn resumed_object(access: &Access, at: &ResumeAt) -> Vec<u8> {
    match access {
        Access::ReverseKey { .. } | Access::ObjectPredicates { .. } => {
            EncodedTerm::id(at.subject).as_bytes().to_vec()
        }
        Access::ForwardKey { .. }
        | Access::ForwardRange { .. }
        | Access::SubjectPredicates { .. }
        | Access::Statement { .. } => at.object.as_bytes().to_vec(),
    }
}

/// A predicate's list's key resumed at: its reader's `after` and what it drops.
pub(super) fn listed_after(access: &Access, at: &ResumeAt) -> Vec<u8> {
    resumed_object(access, at)
}
