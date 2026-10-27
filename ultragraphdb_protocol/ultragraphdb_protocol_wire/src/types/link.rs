/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{NamedNode, NamedOrBlankNode};

/// A link an upsert-and-link writes with the node it resolves: an edge of `predicate` between the node and `other`,
/// the node as its subject or as its object.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Link {
    predicate: NamedNode,
    other: NamedOrBlankNode,
    node_is_subject: bool,
}

impl Link {
    /// The edge `node predicate other`.
    pub fn from_node(predicate: NamedNode, other: NamedOrBlankNode) -> Self {
        Self {
            predicate,
            other,
            node_is_subject: true,
        }
    }

    /// The edge `other predicate node`.
    pub fn to_node(predicate: NamedNode, other: NamedOrBlankNode) -> Self {
        Self {
            predicate,
            other,
            node_is_subject: false,
        }
    }

    pub fn predicate(&self) -> &NamedNode {
        &self.predicate
    }

    pub fn other(&self) -> &NamedOrBlankNode {
        &self.other
    }

    /// Whether the node is the edge's subject; else its object.
    pub fn node_is_subject(&self) -> bool {
        self.node_is_subject
    }
}
