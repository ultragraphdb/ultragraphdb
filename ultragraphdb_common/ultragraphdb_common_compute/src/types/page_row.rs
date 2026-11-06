/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;

/// One row of a page: an object record's object, once per statement ID, or one property of a statement's record.
#[derive(Debug, Clone, Copy)]
pub(super) struct PageRow {
    pub(crate) entry: u32,
    pub(crate) start: u32,
    pub(crate) end: u32,
    pub(crate) statement: Option<SystemId>,
    pub(crate) property: Option<SystemId>,
}
