/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::NamedNode;

/// A relation as the catalog declares it: a type of statement node whose named roles each point at a participant.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RelationDeclaration {
    iri: NamedNode,
    roles: Vec<NamedNode>,
}

impl RelationDeclaration {
    pub fn new(iri: NamedNode, roles: Vec<NamedNode>) -> Self {
        Self { iri, roles }
    }

    pub fn iri(&self) -> &NamedNode {
        &self.iri
    }

    pub fn roles(&self) -> &[NamedNode] {
        &self.roles
    }
}
