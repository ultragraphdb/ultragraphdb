/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/persistent_factory.go (persistentIndexFactory, CreateFactory). Changed for
 * ultragraphdb: translated to Rust; the factory keeps no map of the indexes it made, every index living for one
 * insert or one search as Dgraph's `CreateOrReplace` callers use it, so `find` finds none and `remove` removes
 * nothing; its name and options are the stage-1 registry entry's.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod hnsw_factory_index_factory;

use std::marker::PhantomData;

/// Dgraph's `persistentIndexFactory`: makes the [`PersistentHnsw`](crate::PersistentHnsw) of a vector predicate.
#[derive(Debug, Clone, Copy, Default)]
pub struct HnswFactory<T> {
    float: PhantomData<T>,
}

impl<T> HnswFactory<T> {
    /// Dgraph's `CreateFactory`.
    pub fn new() -> Self {
        Self { float: PhantomData }
    }
}
