/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A link an upsert-and-link writes with the node it resolves: an edge of `predicate` between the node and `other`,
/// both term-table indices of the Operation, the node as its subject or as its object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Link {
    predicate: u32,
    other: u32,
    node_is_subject: bool,
}

impl Link {
    /// The edge `node predicate other`.
    pub fn from_node(predicate: u32, other: u32) -> Self {
        Self {
            predicate,
            other,
            node_is_subject: true,
        }
    }

    /// The edge `other predicate node`.
    pub fn to_node(predicate: u32, other: u32) -> Self {
        Self {
            predicate,
            other,
            node_is_subject: false,
        }
    }

    pub fn predicate(&self) -> u32 {
        self.predicate
    }

    pub fn other(&self) -> u32 {
        self.other
    }

    /// Whether the node is the edge's subject; else its object.
    pub fn node_is_subject(&self) -> bool {
        self.node_is_subject
    }
}
