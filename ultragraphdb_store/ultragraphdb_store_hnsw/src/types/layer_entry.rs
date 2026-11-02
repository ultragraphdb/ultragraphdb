/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// Where a layer's search starts ([`crate::PersistentHnsw`]'s `search_persistent_layer`): the entry node's uid, its
/// vector, and whether the search's filter leaves it out, the three of Dgraph's `searchPersistentLayer` arguments
/// that name the entry.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LayerEntry<'v, T> {
    pub uid: u64,
    pub vector: &'v [T],
    pub filtered_out: bool,
}
