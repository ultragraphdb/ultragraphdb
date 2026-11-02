/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A node of the cluster, named as a partition's preferred leader. Stage 8 places replicas on nodes and enforces
/// the preference; this stage records it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(u32);

impl NodeId {
    /// The node `node`.
    #[must_use]
    pub fn new(node: u32) -> Self {
        Self(node)
    }

    /// The node as a number.
    #[must_use]
    pub fn get(self) -> u32 {
        self.0
    }
}
