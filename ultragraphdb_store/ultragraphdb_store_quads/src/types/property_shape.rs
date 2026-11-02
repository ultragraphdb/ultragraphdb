/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use ultragraphdb_core_mapping::Cardinality;

/// A predicate on the nodes of one label: the kinds of its objects, `iri`, `blank` or a literal's datatype IRI, and
/// its cardinality, single when no node of the label holds two objects of it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PropertyShape {
    kinds: BTreeSet<String>,
    cardinality: Cardinality,
}

impl PropertyShape {
    /// The shape of a predicate whose objects are of `kinds`, with `cardinality`.
    pub fn new(kinds: BTreeSet<String>, cardinality: Cardinality) -> Self {
        Self { kinds, cardinality }
    }

    /// The kinds of its objects: `iri`, `blank` or a literal's datatype IRI.
    pub fn kinds(&self) -> &BTreeSet<String> {
        &self.kinds
    }

    /// Its cardinality: single when no node of the label holds two objects of it.
    pub fn cardinality(&self) -> Cardinality {
        self.cardinality
    }
}
