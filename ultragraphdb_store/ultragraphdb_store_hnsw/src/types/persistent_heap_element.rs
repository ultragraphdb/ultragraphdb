/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/heap.go (persistentHeapElement, initPersistentHeapElement). Changed for
 * ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_ids::SystemId;

/// Dgraph's `persistentHeapElement`: a node's ID with its score, and whether the search's filter left it out of the
/// final result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PersistentHeapElement<T> {
    value: T,
    index: SystemId,
    filtered_out: bool,
}

impl<T: Copy> PersistentHeapElement<T> {
    /// Dgraph's `initPersistentHeapElement`.
    pub fn new(value: T, index: SystemId, filtered_out: bool) -> Self {
        Self {
            value,
            index,
            filtered_out,
        }
    }

    /// The score.
    pub fn value(&self) -> T {
        self.value
    }

    /// The node's ID.
    pub fn index(&self) -> SystemId {
        self.index
    }

    /// Whether the search filter left the node out of the result.
    pub fn filtered_out(&self) -> bool {
        self.filtered_out
    }
}
