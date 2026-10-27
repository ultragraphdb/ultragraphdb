/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use oxrdf::Term;
use ultragraphdb_core_keys::{Keyspace, QuadKey};
use ultragraphdb_core_mapping::{Cardinality, PredicateDeclaration, PredicateKind};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_store::{ListCache, Scan, Store};

use crate::errors::changeset_error::ChangesetError;
use crate::types::changeset_write::ChangesetWrite;
use crate::types::predicate_prefix::PredicatePrefix;

impl<S, C> ChangesetWrite<'_, S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    /// The graph and subject of every forward list of predicate `uid` the write sees: those stored at its read
    /// timestamp, in every graph, and those it wrote. The ranges its skip scan and its scans read are recorded, so that
    /// a commit writing a forward list of the predicate in any graph conflicts with the write.
    fn forward_lists(&mut self, uid: u64) -> Result<BTreeSet<(u64, u64)>, ChangesetError> {
        let ns = self.namespace();
        let read_ts = self.write.read_ts();
        let lists = self.write.quads().lists();
        lists.gate(read_ts)?;
        let snapshot = lists.store().snapshot(read_ts)?;
        let mut keys: Vec<Vec<u8>> = Vec::new();
        let prefix = PredicatePrefix::new(Keyspace::Forward, false, ns, uid, Vec::new());
        let (graphs, ranges) = prefix.graphs_read(&snapshot)?;
        let request = self.write.request();
        for (start, end) in ranges {
            request.record_range(start, end);
        }
        for graph in graphs {
            let graph_prefix = prefix.in_graph(graph);
            let end = Scan::prefix(graph_prefix.clone()).end().map(<[u8]>::to_vec);
            keys.extend(request.scan(graph_prefix, end)?);
        }
        keys.extend(request.written_keys().map(<[u8]>::to_vec));
        Ok(keys
            .iter()
            .filter_map(|k| match QuadKey::parse(k) {
                Ok((
                    QuadKey::Forward {
                        namespace,
                        graph,
                        predicate,
                        subject,
                    },
                    None,
                )) if namespace == ns && predicate == uid => Some((graph, subject)),
                _ => None,
            })
            .collect())
    }

    /// Whether predicate `uid` holds an object in some graph: one of its forward lists is not empty.
    pub(crate) fn holds_data(&mut self, uid: u64) -> Result<bool, ChangesetError> {
        for (graph, subject) in self.forward_lists(uid)? {
            if !self.write.objects(graph, subject, uid)?.is_empty() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Checks predicate `uid`'s data against `new` where it narrows `old`: a single-valued predicate whose subject holds
    /// two objects, an edge holding a literal, a property holding an IRI or blank node, a datatype constraint a literal
    /// contradicts. The refusal names the predicate and the subject at fault.
    pub(crate) fn check_data(
        &mut self,
        uid: u64,
        old: &PredicateDeclaration,
        new: &PredicateDeclaration,
    ) -> Result<(), ChangesetError> {
        let single =
            new.cardinality() == Cardinality::Single && old.cardinality() != Cardinality::Single;
        let edge = new.kind() == PredicateKind::Edge && old.kind() != PredicateKind::Edge;
        let property =
            new.kind() == PredicateKind::Property && old.kind() != PredicateKind::Property;
        let datatype = new.datatype().filter(|d| old.datatype() != Some(*d));
        if !single && !edge && !property && datatype.is_none() {
            return Ok(());
        }
        let iri = new.predicate().as_str();
        for (graph, subject) in self.forward_lists(uid)? {
            let objects: Vec<EncodedTerm> = self
                .write
                .objects(graph, subject, uid)?
                .into_iter()
                .map(|(o, _)| o)
                .collect();
            let fault = if single && objects.len() > 1 {
                Some("holds more than one object")
            } else if edge && objects.iter().any(|o| o.as_uid().is_none()) {
                Some("holds a literal")
            } else if property && objects.iter().any(|o| o.as_uid().is_some()) {
                Some("holds an IRI or a blank node")
            } else {
                None
            };
            let fault = match (fault, datatype) {
                (Some(f), _) => Some(f),
                (None, Some(datatype)) => {
                    let mut contradicts = false;
                    for object in objects.iter().filter(|o| o.as_uid().is_none()) {
                        if let Term::Literal(l) = self.decode(object)? {
                            contradicts |= l.datatype() != datatype.as_ref();
                        }
                    }
                    contradicts.then_some("holds a literal of another datatype")
                }
                (None, None) => None,
            };
            if let Some(reason) = fault {
                let subject = self.decode(&EncodedTerm::uid(subject))?.to_string();
                return Err(ChangesetError::Contradicted(iri, subject, reason));
            }
        }
        Ok(())
    }
}
