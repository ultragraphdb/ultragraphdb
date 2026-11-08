/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::vocab::rdf;
use ultragraphdb_core_catalog::Catalog;
use ultragraphdb_core_keys::{Graph, Keyspace, QuadKey, key_prefix};
use ultragraphdb_core_mapping::{Cardinality, PredicateDeclaration, PredicateKind};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{ListCache, Scan, Store, StoreSnapshot};

use crate::errors::quad_error::QuadError;
use crate::types::data_dictionary::DataDictionary;
use crate::types::description::Description;
use crate::types::property_shape::PropertyShape;
use crate::types::quad_store::QuadStore;
use crate::types::statistics::Statistics;

/// The statistics kinds of objects that are nodes.
const NODE_KINDS: [&str; 2] = ["iri", "blank"];

impl DataDictionary {
    /// What the tenant holds at `read_ts`: the catalog's declarations at that snapshot, exact; the statistics of
    /// each graph and predicate, exact at it; and the last pass's shapes, each declared predicate on the entity types
    /// it is declared on set as declared.
    ///
    /// # Errors
    ///
    /// The store's error; corruption of a catalog entry, a statistics key or entry, or a dictionary entry.
    pub fn describe<S, C, B>(
        &self,
        quads: &QuadStore<S, C, B>,
        read_ts: u64,
    ) -> Result<Description, QuadError>
    where
        S: Store,
        C: ListCache<Value = CachedList>,
        B: BlockSource,
    {
        let tenant = self.tenant;
        let snapshot = quads.store().snapshot(read_ts)?;
        let catalog = Catalog::load(&snapshot, tenant)?;
        let mut d = Description::from_shapes(self.shapes().as_ref());
        for item in snapshot.scan(&Scan::prefix(key_prefix(
            Keyspace::Statistics,
            false,
            tenant,
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
                let statistics = quads.statistics(tenant, graph, predicate, read_ts)?;
                let name = quads.iri_of(tenant, predicate)?.0;
                let graph_name = match graph {
                    Graph::Default => String::new(),
                    Graph::Named(g) => quads.iri_of(tenant, g)?.0,
                };
                if name != rdf::TYPE.as_str() {
                    let counted = |kind: &str| statistics.kinds().get(kind).is_some_and(|n| *n > 0);
                    if NODE_KINDS.iter().any(|k| counted(k)) {
                        let _new_relationship_type: bool =
                            d.relationship_types.insert(name.clone());
                    }
                    if statistics
                        .kinds()
                        .keys()
                        .any(|k| !NODE_KINDS.contains(&k.as_str()) && counted(k))
                    {
                        let _new_property_key: bool = d.property_keys.insert(name.clone());
                    }
                }
                let _replaced_statistics: Option<Statistics> =
                    d.statistics.insert((graph_name, name), statistics);
            }
        }
        for (_, declaration) in catalog.predicates() {
            describe_predicate(&mut d, declaration);
        }
        let preorder = catalog.preorder();
        for (uid, t) in catalog.entity_types() {
            let _new_label: bool = d.labels.insert(t.iri().as_str().to_string());
            d.entity_types
                .push((t.clone(), preorder.get(&uid).copied().unwrap_or_default()));
        }
        d.relations = catalog.relations().map(|(_, r)| r.clone()).collect();
        Ok(d)
    }
}

/// Adds `declaration` to `d`: its name as a relationship type or property key by its kind, its domains as labels with
/// the shape of the property on each, and the declaration itself.
fn describe_predicate(d: &mut Description, declaration: &PredicateDeclaration) {
    let name = declaration.predicate().as_str().to_string();
    match declaration.kind() {
        PredicateKind::Edge if name != rdf::TYPE.as_str() => {
            let _new_relationship_type: bool = d.relationship_types.insert(name.clone());
        }
        PredicateKind::Property => {
            let _new_property_key: bool = d.property_keys.insert(name.clone());
        }
        PredicateKind::Edge | PredicateKind::Either => {}
    }
    for domain in declaration.domains() {
        let _new_label: bool = d.labels.insert(domain.as_str().to_string());
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
        let _replaced_shape: Option<PropertyShape> = d
            .shapes
            .entry(domain.as_str().to_string())
            .or_default()
            .properties
            .insert(name.clone(), PropertyShape::new(kinds, cardinality));
    }
    d.predicates.push(declaration.clone());
}
