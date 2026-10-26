/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::vocab::rdf;
use ultragraphdb_core_catalog::Catalog;
use ultragraphdb_core_keys::{DEFAULT_GRAPH, Keyspace, QuadKey, key_prefix};
use ultragraphdb_core_mapping::{Cardinality, PredicateKind};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_store::{ListCache, Scan, Store, StoreSnapshot};

use crate::errors::quad_error::QuadError;
use crate::types::data_dictionary::DataDictionary;
use crate::types::description::Description;
use crate::types::property_shape::PropertyShape;
use crate::types::quad_store::QuadStore;

/// The statistics kinds of objects that are nodes.
const NODE_KINDS: [&str; 2] = ["iri", "blank"];

impl DataDictionary {
    /// What the namespace holds at `read_ts`: the catalog's declarations at that snapshot, exact; the statistics of
    /// each graph and predicate, exact at it; and the last pass's shapes, each declared predicate on the entity types
    /// it is declared on set as declared.
    pub fn describe<S, C>(
        &self,
        quads: &QuadStore<S, C>,
        read_ts: u64,
    ) -> Result<Description, QuadError>
    where
        S: Store,
        C: ListCache<Value = CachedList>,
    {
        let ns = self.namespace;
        let snapshot = quads.store().snapshot(read_ts)?;
        let catalog = Catalog::load(&snapshot, ns)?;
        let mut d = Description::from_shapes(self.shapes().as_ref());
        for item in snapshot.scan(&Scan::prefix(key_prefix(
            Keyspace::Statistics,
            false,
            ns,
            None,
            None,
        ))) {
            let key = item?.key().to_vec();
            if let (
                QuadKey::Statistics {
                    graph, predicate, ..
                },
                _,
            ) = QuadKey::parse(&key)?
            {
                let statistics = quads.statistics(ns, graph, predicate, read_ts)?;
                let name = quads.iri_of(ns, predicate)?.0;
                let graph_name = match graph {
                    DEFAULT_GRAPH => String::new(),
                    g => quads.iri_of(ns, g)?.0,
                };
                if name != rdf::TYPE.as_str() {
                    let counted = |kind: &str| statistics.kinds().get(kind).is_some_and(|n| *n > 0);
                    if NODE_KINDS.iter().any(|k| counted(k)) {
                        d.relationship_types.insert(name.clone());
                    }
                    if statistics
                        .kinds()
                        .keys()
                        .any(|k| !NODE_KINDS.contains(&k.as_str()) && counted(k))
                    {
                        d.property_keys.insert(name.clone());
                    }
                }
                d.statistics.insert((graph_name, name), statistics);
            }
        }
        for (_, declaration) in catalog.predicates() {
            let name = declaration.predicate().as_str().to_string();
            match declaration.kind() {
                PredicateKind::Edge if name != rdf::TYPE.as_str() => {
                    d.relationship_types.insert(name.clone());
                }
                PredicateKind::Property => {
                    d.property_keys.insert(name.clone());
                }
                _ => {}
            }
            for domain in declaration.domains() {
                d.labels.insert(domain.as_str().to_string());
                let kinds = declaration
                    .datatype()
                    .map(|t| [t.as_str().to_string()].into())
                    .unwrap_or_else(|| {
                        d.shapes
                            .get(domain.as_str())
                            .and_then(|s| s.properties.get(&name))
                            .map(|p| p.kinds().clone())
                            .unwrap_or_default()
                    });
                let cardinality: Cardinality = declaration.cardinality();
                d.shapes
                    .entry(domain.as_str().to_string())
                    .or_default()
                    .properties
                    .insert(name.clone(), PropertyShape::new(kinds, cardinality));
            }
            d.predicates.push(declaration.clone());
        }
        let preorder = catalog.preorder();
        for (uid, t) in catalog.entity_types() {
            d.labels.insert(t.iri().as_str().to_string());
            d.entity_types
                .push((t.clone(), preorder.get(&uid).copied().unwrap_or_default()));
        }
        d.relations = catalog.relations().map(|(_, r)| r.clone()).collect();
        Ok(d)
    }
}
