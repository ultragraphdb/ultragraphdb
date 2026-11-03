/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, BTreeSet};

use oxrdf::Term;
use oxrdf::vocab::rdf;
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{Graph, Keyspace, QuadKey, key_prefix};
use ultragraphdb_core_mapping::Cardinality;
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{ListCache, Scan, Store, StoreSnapshot};

use crate::errors::quad_error::QuadError;
use crate::types::property_shape::PropertyShape;
use crate::types::quad_store::QuadStore;
use crate::types::shapes::Shapes;

/// The kinds of each predicate a label's nodes hold, and the most objects one node holds of it.
type Observed = BTreeMap<String, (BTreeSet<String>, usize)>;

/// Each typed subject's labels, by its graph and subject.
type Typed = BTreeMap<(Graph, SystemId), Vec<String>>;

/// The labels of the objects of each predicate a label's nodes hold, by label and predicate.
type Related = BTreeMap<String, BTreeMap<String, BTreeSet<String>>>;

impl Shapes {
    /// The shapes of `namespace` at `read_ts`: each subject's labels from its `rdf:type` statements, joined with its
    /// subject-predicate list, and for each IRI object the labels of the object in the same graph.
    pub fn compute<S, C, B>(
        quads: &QuadStore<S, C, B>,
        namespace: u64,
        read_ts: u64,
    ) -> Result<Self, QuadError>
    where
        S: Store,
        C: ListCache<Value = CachedList>,
        B: BlockSource,
    {
        let mut shapes = Self {
            read_ts,
            labels: BTreeMap::new(),
        };
        let Some(rdf_type) = quads.id_of(namespace, rdf::TYPE.as_str())? else {
            return Ok(shapes);
        };
        // Held to the end of the pass, so that what it reads stays at `read_ts`.
        let snapshot = quads.store().snapshot(read_ts)?;
        let typed = typed_subjects(quads, &snapshot, namespace, read_ts, rdf_type)?;
        let (properties, relationships) =
            observe_predicates(quads, namespace, read_ts, rdf_type, &typed)?;
        for label in typed.values().flatten() {
            let shape = shapes.labels.entry(label.clone()).or_default();
            for (name, (kinds, most)) in properties.get(label).into_iter().flatten() {
                let cardinality = if *most > 1 {
                    Cardinality::Multi
                } else {
                    Cardinality::Single
                };
                let _replaced_property: Option<PropertyShape> = shape
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

/// The labels of each subject of `namespace` at `read_ts` from its `rdf_type` statements, by graph and subject, its
/// forward lists found through `snapshot`.
fn typed_subjects<S, C, B>(
    quads: &QuadStore<S, C, B>,
    snapshot: &S::Snapshot,
    namespace: u64,
    read_ts: u64,
    rdf_type: SystemId,
) -> Result<Typed, QuadError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    B: BlockSource,
{
    let mut typed = Typed::new();
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
                .filter_map(|(o, _)| o.as_id())
                .map(|u| quads.iri_of(namespace, u).map(|(iri, _)| iri))
                .collect::<Result<Vec<_>, _>>()?;
            if !labels.is_empty() {
                let _replaced_labels: Option<Vec<String>> = typed.insert((graph, subject), labels);
            }
        }
    }
    Ok(typed)
}

/// What the `typed` subjects' predicates other than `rdf_type` hold at `read_ts`, by label: each one's kinds and most
/// objects, and the labels of its IRI objects in the same graph.
fn observe_predicates<S, C, B>(
    quads: &QuadStore<S, C, B>,
    namespace: u64,
    read_ts: u64,
    rdf_type: SystemId,
    typed: &Typed,
) -> Result<(BTreeMap<String, Observed>, Related), QuadError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    B: BlockSource,
{
    let mut properties: BTreeMap<String, Observed> = BTreeMap::new();
    let mut relationships = Related::new();
    for ((graph, subject), labels) in typed {
        let predicates = QuadKey::SubjectPredicates {
            namespace,
            graph: *graph,
            subject: *subject,
        }
        .to_bytes();
        for (p, _) in quads.objects(&predicates, read_ts)? {
            let Some(predicate) = p.as_id().filter(|p| *p != rdf_type) else {
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
                let _new_kind: bool = kinds.insert(kind(quads, namespace, object)?);
                if let Some(uid) = object.as_id() {
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
    Ok((properties, relationships))
}

/// The statistics kind of a stored object: `iri`, `blank` or its datatype IRI.
fn kind<S, C, B>(
    quads: &QuadStore<S, C, B>,
    namespace: u64,
    object: &EncodedTerm,
) -> Result<String, QuadError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    B: BlockSource,
{
    Ok(match quads.term_of(namespace, object)? {
        Term::NamedNode(_) => "iri".to_string(),
        Term::BlankNode(_) => "blank".to_string(),
        Term::Literal(l) => l.datatype().as_str().to_string(),
        Term::Triple(_) => "triple".to_string(),
    })
}
