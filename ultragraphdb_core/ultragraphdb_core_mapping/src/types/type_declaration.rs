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
    /// The declaration of the entity type `iri`, with its parent and whether it is abstract.
    pub fn new(iri: NamedNode, parent: Option<NamedNode>, is_abstract: bool) -> Self {
        Self {
            iri,
            parent,
            is_abstract,
        }
    }

    /// The type's IRI.
    pub fn iri(&self) -> &NamedNode {
        &self.iri
    }

    /// The parent type, when it has one.
    pub fn parent(&self) -> Option<&NamedNode> {
        self.parent.as_ref()
    }

    /// Whether only its subtypes have instances.
    pub fn is_abstract(&self) -> bool {
        self.is_abstract
    }
}
