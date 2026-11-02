/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_store::PersistMode;

use crate::utils::constants::MAX_LIST_SIZE;

/// How [`crate::Lists`] writes: the persist mode of its commit batches, `Buffer` for Dgraph's `process`
/// durability, whose owner syncs once a minute, and `Sync` for `filesystem`; and the size at which a rollup splits
/// a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListsConfig {
    persist: PersistMode,
    max_list_size: usize,
}

impl ListsConfig {
    /// A configuration writing with `persist` and splitting a list at `max_list_size`.
    pub fn new(persist: PersistMode, max_list_size: usize) -> Self {
        Self {
            persist,
            max_list_size,
        }
    }

    /// The persist mode of the commit batches.
    pub fn persist(&self) -> PersistMode {
        self.persist
    }

    /// The size at which a rollup splits a list.
    pub fn max_list_size(&self) -> usize {
        self.max_list_size
    }
}

impl Default for ListsConfig {
    /// `process` durability and Dgraph's split size.
    fn default() -> Self {
        Self::new(PersistMode::Buffer, MAX_LIST_SIZE)
    }
}
