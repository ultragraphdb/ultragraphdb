/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::vocab::rdf;
use oxrdf::{GraphName, Literal, NamedNode, NamedOrBlankNode, Term};

use crate::types::cardinality::Cardinality;
use crate::types::change::Change;
use crate::types::changeset::Changeset;
use crate::types::names::Names;
use crate::types::pg_edge::PgEdge;
use crate::types::pg_node::PgNode;
use crate::types::pg_value::PgValue;
use crate::types::property_graph::PropertyGraph;
use crate::types::statement::Statement;

/// The property-graph view of `graph`'s `statements`: each subject and each IRI or blank-node object a node; an
/// `rdf:type` statement with an IRI object a label of its subject; a statement with a literal object a property of
/// its subject, scalar when `cardinality` says its predicate is single-valued and a list otherwise; every other
/// statement an edge, a triple held by several statements one edge for each, with its statement ID and properties.
pub fn property_graph_view<'a>(
    statements: impl IntoIterator<Item = &'a Statement>,
    graph: &GraphName,
    names: &Names,
    cardinality: impl Fn(&NamedNode) -> Cardinality,
) -> PropertyGraph {
    let mut pg = PropertyGraph::new();
    for s in statements.into_iter().filter(|s| s.graph() == graph) {
        let node = pg.node_mut(PgNode::new(s.subject().clone()));
        match (s.object(), s.predicate() == &rdf::TYPE) {
            (Term::NamedNode(label), true) => {
                node.labels_mut().insert(names.name(label));
            }
            (Term::Literal(literal), _) => {
                let name = names.name(s.predicate());
                let single = cardinality(s.predicate()) == Cardinality::Single;
                let value = node
                    .properties_mut()
                    .entry(name)
                    .or_insert_with(|| PgValue::List(Vec::new()));
                match value {
                    PgValue::List(values) if !single => {
                        values.push(literal.clone());
                        values.sort_by_key(Literal::to_string);
                    }
                    _ => *value = PgValue::Scalar(literal.clone()),
                }
            }
            (object, _) => {
                let Some(target) = node_of(object) else {
                    continue;
                };
                pg.node_mut(PgNode::new(target.clone()));
                let mut edge = PgEdge::new(s.subject().clone(), names.name(s.predicate()), target);
                for (predicate, _) in s.properties() {
                    let values = s
                        .properties()
                        .iter()
                        .filter(|(p, _)| p == predicate)
                        .map(|(_, v)| v.clone())
                        .collect();
                    edge = edge.with_property(names.name(predicate), values);
                }
                if let Some(id) = s.id() {
                    edge = edge.with_id(id);
                }
                pg.push_edge(edge);
            }
        }
    }
    pg.sort_edges();
    pg
}

/// The changeset that writes `pg` into `graph` through the property-graph view: each label an `rdf:type`
/// statement, each property value a statement with a literal object, and each edge a statement of its own with its
/// properties.
pub fn property_graph_to_changeset(
    pg: &PropertyGraph,
    namespace: u64,
    graph: &GraphName,
    names: &Names,
) -> Changeset {
    let mut changeset = Changeset::new(namespace);
    for node in pg.nodes() {
        for label in node.labels() {
            changeset.push(Change::Insert {
                graph: graph.clone(),
                subject: node.id().clone(),
                predicate: rdf::TYPE.into_owned(),
                object: names.iri(label).into(),
            });
        }
        for (name, value) in node.properties() {
            let values = match value {
                PgValue::Scalar(v) => std::slice::from_ref(v),
                PgValue::List(vs) => vs.as_slice(),
            };
            for v in values {
                changeset.push(Change::Insert {
                    graph: graph.clone(),
                    subject: node.id().clone(),
                    predicate: names.iri(name),
                    object: v.clone().into(),
                });
            }
        }
    }
    for edge in pg.edges() {
        let properties = edge
            .properties()
            .iter()
            .flat_map(|(name, values)| values.iter().map(move |v| (names.iri(name), v.clone())))
            .collect();
        changeset.push(Change::InsertEdge {
            graph: graph.clone(),
            subject: edge.source().clone(),
            predicate: names.iri(edge.edge_type()),
            object: Term::from(edge.target().clone()),
            properties,
        });
    }
    changeset
}

fn node_of(object: &Term) -> Option<NamedOrBlankNode> {
    match object {
        Term::NamedNode(n) => Some(n.clone().into()),
        Term::BlankNode(b) => Some(b.clone().into()),
        _ => None,
    }
}
