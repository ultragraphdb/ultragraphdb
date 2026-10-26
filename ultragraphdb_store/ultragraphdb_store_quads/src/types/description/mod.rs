/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ultragraphdb_core_mapping::{PredicateDeclaration, RelationDeclaration, TypeDeclaration};

use crate::types::label_shape::LabelShape;
use crate::types::shapes::Shapes;
use crate::types::statistics::Statistics;

/// What a namespace holds at a snapshot, the data dictionary's answer: its labels, relationship types and property
/// keys; each label's shape; the catalog's declarations, its entity types with their preorder numbers; and the
/// statistics of each graph and predicate, by the graph's IRI, empty for the default graph, and the predicate's.
/// Declared entries are exact; the shapes of undeclared data are the last pass's, at [`Description::shapes_ts`].
#[derive(Debug, Clone, PartialEq)]
pub struct Description {
    pub(crate) labels: BTreeSet<String>,
    pub(crate) relationship_types: BTreeSet<String>,
    pub(crate) property_keys: BTreeSet<String>,
    pub(crate) shapes: BTreeMap<String, LabelShape>,
    pub(crate) shapes_ts: Option<u64>,
    pub(crate) predicates: Vec<PredicateDeclaration>,
    pub(crate) entity_types: Vec<(TypeDeclaration, u32)>,
    pub(crate) relations: Vec<RelationDeclaration>,
    pub(crate) statistics: BTreeMap<(String, String), Statistics>,
}

impl Description {
    pub(crate) fn from_shapes(shapes: Option<&Arc<Shapes>>) -> Self {
        Self {
            labels: shapes
                .iter()
                .flat_map(|s| s.labels().keys().cloned())
                .collect(),
            relationship_types: BTreeSet::new(),
            property_keys: BTreeSet::new(),
            shapes: shapes.map(|s| s.labels().clone()).unwrap_or_default(),
            shapes_ts: shapes.map(|s| s.read_ts()),
            predicates: Vec::new(),
            entity_types: Vec::new(),
            relations: Vec::new(),
            statistics: BTreeMap::new(),
        }
    }

    pub fn labels(&self) -> &BTreeSet<String> {
        &self.labels
    }

    pub fn relationship_types(&self) -> &BTreeSet<String> {
        &self.relationship_types
    }

    pub fn property_keys(&self) -> &BTreeSet<String> {
        &self.property_keys
    }

    /// Each label's shape: the last pass's, with the catalog's declarations over it.
    pub fn shapes(&self) -> &BTreeMap<String, LabelShape> {
        &self.shapes
    }

    /// The snapshot timestamp of the pass the shapes come from; `None` before the first pass.
    pub fn shapes_ts(&self) -> Option<u64> {
        self.shapes_ts
    }

    /// The predicates the catalog declares, with their indexes and constraints.
    pub fn predicates(&self) -> &[PredicateDeclaration] {
        &self.predicates
    }

    pub fn entity_types(&self) -> &[(TypeDeclaration, u32)] {
        &self.entity_types
    }

    pub fn relations(&self) -> &[RelationDeclaration] {
        &self.relations
    }

    pub fn statistics(&self) -> &BTreeMap<(String, String), Statistics> {
        &self.statistics
    }
}
