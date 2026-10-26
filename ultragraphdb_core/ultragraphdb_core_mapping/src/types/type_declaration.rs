/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::NamedNode;

/// An entity type as the catalog declares it: at most one parent, and whether it is abstract, so that only its
/// subtypes have instances.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeDeclaration {
    iri: NamedNode,
    parent: Option<NamedNode>,
    is_abstract: bool,
}

impl TypeDeclaration {
    pub fn new(iri: NamedNode, parent: Option<NamedNode>, is_abstract: bool) -> Self {
        Self {
            iri,
            parent,
            is_abstract,
        }
    }

    pub fn iri(&self) -> &NamedNode {
        &self.iri
    }

    pub fn parent(&self) -> Option<&NamedNode> {
        self.parent.as_ref()
    }

    pub fn is_abstract(&self) -> bool {
        self.is_abstract
    }
}
