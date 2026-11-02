/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;

/// An element of an [`IdHeap`](crate::IdHeap): an ID and the list it comes from. Not `Copy`: at 24 bytes it is above
/// Ultra Style's 16 for a copied value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Elem {
    val: SystemId,
    list_idx: usize,
}

impl Elem {
    /// The element `val` from the list at `list_idx`.
    #[must_use]
    pub fn new(val: SystemId, list_idx: usize) -> Self {
        Self { val, list_idx }
    }

    /// The element's ID.
    #[must_use]
    pub fn val(&self) -> SystemId {
        self.val
    }

    /// The index of the list the ID comes from.
    #[must_use]
    pub fn list_idx(&self) -> usize {
        self.list_idx
    }
}
