/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// An entity type's declaration, by IRI text: its parent, if any, and whether it is abstract.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeclaredEntityType {
    iri: String,
    parent: Option<String>,
    is_abstract: bool,
}

impl DeclaredEntityType {
    /// The declaration of the entity type `iri`, below `parent`, abstract or not.
    pub fn new(iri: impl Into<String>, parent: Option<String>, is_abstract: bool) -> Self {
        Self {
            iri: iri.into(),
            parent,
            is_abstract,
        }
    }

    /// The entity type's IRI.
    pub fn iri(&self) -> &str {
        &self.iri
    }

    /// The parent type's IRI, if it has one.
    pub fn parent(&self) -> Option<&str> {
        self.parent.as_deref()
    }

    /// Whether the entity type is abstract.
    pub fn is_abstract(&self) -> bool {
        self.is_abstract
    }
}
