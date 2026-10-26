/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, BTreeSet};

use crate::types::property_shape::PropertyShape;

/// What the nodes of one label hold: each predicate with its shape, and each relationship type with the labels of
/// the nodes it leads to.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LabelShape {
    pub(crate) properties: BTreeMap<String, PropertyShape>,
    pub(crate) relationships: BTreeMap<String, BTreeSet<String>>,
}

impl LabelShape {
    pub fn properties(&self) -> &BTreeMap<String, PropertyShape> {
        &self.properties
    }

    pub fn relationships(&self) -> &BTreeMap<String, BTreeSet<String>> {
        &self.relationships
    }
}
