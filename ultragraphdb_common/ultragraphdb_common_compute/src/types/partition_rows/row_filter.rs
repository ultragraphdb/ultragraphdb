/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;

/// What only some of a list's or record's objects are read for: a bound object beside a list's key, or a bound predicate and
/// object in a statement's record.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RowFilter {
    pub(crate) object: Option<Vec<u8>>,
    pub(crate) property: Option<SystemId>,
}
