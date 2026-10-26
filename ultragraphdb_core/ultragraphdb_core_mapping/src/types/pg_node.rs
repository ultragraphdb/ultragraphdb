/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, BTreeSet};

use oxrdf::NamedOrBlankNode;

use crate::types::pg_value::PgValue;

/// A node of the property-graph view: an IRI or blank node, its labels and its properties, each by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PgNode {
    id: NamedOrBlankNode,
    labels: BTreeSet<String>,
    properties: BTreeMap<String, PgValue>,
}

impl PgNode {
    pub fn new(id: NamedOrBlankNode) -> Self {
        Self {
            id,
            labels: BTreeSet::new(),
            properties: BTreeMap::new(),
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.labels.insert(label.into());
        self
    }

    pub fn with_property(mut self, name: impl Into<String>, value: PgValue) -> Self {
        self.properties.insert(name.into(), value);
        self
    }

    pub fn id(&self) -> &NamedOrBlankNode {
        &self.id
    }

    pub fn labels(&self) -> &BTreeSet<String> {
        &self.labels
    }

    pub fn properties(&self) -> &BTreeMap<String, PgValue> {
        &self.properties
    }

    pub(crate) fn labels_mut(&mut self) -> &mut BTreeSet<String> {
        &mut self.labels
    }

    pub(crate) fn properties_mut(&mut self) -> &mut BTreeMap<String, PgValue> {
        &mut self.properties
    }
}
