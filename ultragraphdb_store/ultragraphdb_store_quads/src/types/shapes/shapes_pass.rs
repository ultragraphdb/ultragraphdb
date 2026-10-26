/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, BTreeSet};

use oxrdf::Term;
use oxrdf::vocab::rdf;
use ultragraphdb_core_keys::{Keyspace, QuadKey, key_prefix};
use ultragraphdb_core_mapping::Cardinality;
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_store::{ListCache, Scan, Store, StoreSnapshot};

use crate::errors::quad_error::QuadError;
use crate::types::property_shape::PropertyShape;
use crate::types::quad_store::QuadStore;
use crate::types::shapes::Shapes;

/// The kinds of each predicate a label's nodes hold, and the most objects one node holds of it.
type Observed = BTreeMap<String, (BTreeSet<String>, usize)>;

impl Shapes {
    /// The shapes of `namespace` at `read_ts`: each subject's labels from its `rdf:type` statements, joined with its
    /// subject-predicate list, and for each IRI object the labels of the object in the same graph.
    pub fn compute<S, C>(
        quads: &QuadStore<S, C>,
        namespace: u64,
        read_ts: u64,
    ) -> Result<Self, QuadError>
    where
        S: Store,
        C: ListCache<Value = CachedList>,
    {
        let mut shapes = Self {
            read_ts,
            labels: BTreeMap::new(),
        };
        let Some(rdf_type) = quads.uid_of(namespace, rdf::TYPE.as_str())? else {
            return Ok(shapes);
        };
        let snapshot = quads.store().snapshot(read_ts)?;
        let mut typed: BTreeMap<(u64, u64), Vec<String>> = BTreeMap::new();
        for item in snapshot.scan(&Scan::prefix(key_prefix(
            Keyspace::Forward,
            false,
            namespace,
            None,
            None,
        ))) {
            let key = item?.key().to_vec();
            if let (
                QuadKey::Forward {
                    graph,
                    predicate,
                    subject,
                    ..
                },
                _,
            ) = QuadKey::parse(&key)?
                && predicate == rdf_type
            {
                let labels = quads
                    .objects(&key, read_ts)?
                    .iter()
                    .filter_map(|(o, _)| o.as_uid())
                    .map(|u| quads.iri_of(namespace, u).map(|(iri, _)| iri))
                    .collect::<Result<Vec<_>, _>>()?;
                if !labels.is_empty() {
                    typed.insert((graph, subject), labels);
                }
            }
        }
        let mut properties: BTreeMap<String, Observed> = BTreeMap::new();
        let mut relationships: BTreeMap<String, BTreeMap<String, BTreeSet<String>>> =
            BTreeMap::new();
        for ((graph, subject), labels) in &typed {
            let predicates = QuadKey::SubjectPredicates {
                namespace,
                graph: *graph,
                subject: *subject,
            }
            .to_bytes();
            for (p, _) in quads.objects(&predicates, read_ts)? {
                let Some(predicate) = p.as_uid().filter(|p| *p != rdf_type) else {
                    continue;
                };
                let name = quads.iri_of(namespace, predicate)?.0;
                let forward = QuadKey::Forward {
                    namespace,
                    graph: *graph,
                    predicate,
                    subject: *subject,
                }
                .to_bytes();
                let objects = quads.objects(&forward, read_ts)?;
                let mut kinds = BTreeSet::new();
                let mut targets = BTreeSet::new();
                for (object, _) in &objects {
                    kinds.insert(kind(quads, namespace, object)?);
                    if let Some(uid) = object.as_uid() {
                        targets.extend(typed.get(&(*graph, uid)).into_iter().flatten().cloned());
                    }
                }
                for label in labels {
                    let entry = properties
                        .entry(label.clone())
                        .or_default()
                        .entry(name.clone())
                        .or_default();
                    entry.0.extend(kinds.iter().cloned());
                    entry.1 = entry.1.max(objects.len());
                    if !targets.is_empty() {
                        relationships
                            .entry(label.clone())
                            .or_default()
                            .entry(name.clone())
                            .or_default()
                            .extend(targets.iter().cloned());
                    }
                }
            }
        }
        for label in typed.values().flatten() {
            let shape = shapes.labels.entry(label.clone()).or_default();
            for (name, (kinds, most)) in properties.get(label).into_iter().flatten() {
                let cardinality = if *most > 1 {
                    Cardinality::Multi
                } else {
                    Cardinality::Single
                };
                shape
                    .properties
                    .insert(name.clone(), PropertyShape::new(kinds.clone(), cardinality));
            }
            if let Some(r) = relationships.get(label) {
                shape.relationships = r.clone();
            }
        }
        Ok(shapes)
    }
}

/// The statistics kind of a stored object: `iri`, `blank` or its datatype IRI.
fn kind<S, C>(
    quads: &QuadStore<S, C>,
    namespace: u64,
    object: &EncodedTerm,
) -> Result<String, QuadError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    Ok(match quads.term_of(namespace, object)? {
        Term::NamedNode(_) => "iri".to_string(),
        Term::BlankNode(_) => "blank".to_string(),
        Term::Literal(l) => l.datatype().as_str().to_string(),
        Term::Triple(_) => "triple".to_string(),
    })
}
