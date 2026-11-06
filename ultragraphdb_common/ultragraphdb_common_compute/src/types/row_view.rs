/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cmp::Ordering;

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::QuadKey;

/// A row of a page, read in place.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RowView<'a> {
    pub(crate) key: &'a [u8],
    pub(crate) parsed: &'a QuadKey,
    pub(crate) encoding: &'a [u8],
    pub(crate) statement: Option<SystemId>,
    /// A statement's record's row: the property's predicate.
    pub(crate) property: Option<SystemId>,
}

impl RowView<'_> {
    /// The rows' order across partitions: the store's key order, then the list's, one object's by ascending statement ID.
    pub(crate) fn order(&self, other: &RowView<'_>) -> Ordering {
        self.key
            .cmp(other.key)
            .then_with(|| self.encoding.cmp(other.encoding))
            .then_with(|| self.statement.cmp(&other.statement))
    }
}
