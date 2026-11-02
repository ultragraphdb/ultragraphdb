/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;

/// A plain list of system IDs, sorted where the set operations need it: Dgraph's `pb.List`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct List {
    ids: Vec<SystemId>,
}

impl List {
    /// A list of `ids`.
    #[must_use]
    pub fn new(ids: Vec<SystemId>) -> Self {
        Self { ids }
    }

    /// The IDs.
    #[must_use]
    pub fn ids(&self) -> &[SystemId] {
        &self.ids
    }

    /// The IDs, for the set operations that write their result in place.
    pub fn ids_mut(&mut self) -> &mut Vec<SystemId> {
        &mut self.ids
    }

    /// The IDs, taken out of the list.
    #[must_use]
    pub fn into_ids(self) -> Vec<SystemId> {
        self.ids
    }
}
