/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::{Graph, Keyspace};

/// The graphs a partition's read goes through.
#[derive(Debug)]
pub(super) enum Graphs {
    /// This graph alone, then none.
    One(Graph),
    /// Walked by Scans of limit 1 from this key of the keyspace.
    Walk { keyspace: Keyspace, from: Vec<u8> },
    /// None left.
    Done,
}
