/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;

/// The graph a key names: the default graph, which a statement belongs to when it names none, or a named graph by
/// its system ID. A key writes a named graph as its ID's 16 big-endian bytes and the default graph as 16 zero bytes,
/// an encoding no system ID takes, since a system ID's type prefix is never 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Graph {
    /// The default graph.
    Default,
    /// A named graph, by its system ID.
    Named(SystemId),
}

impl Graph {
    /// The graph's 16 bytes in a key.
    #[must_use]
    pub fn to_be_bytes(self) -> [u8; 16] {
        match self {
            Graph::Default => [0; 16],
            Graph::Named(id) => id.to_be_bytes(),
        }
    }
}
