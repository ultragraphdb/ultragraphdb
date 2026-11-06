/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::entries_end::EntriesEnd;

/// The frame before the one being checked: what the rules comparing two frames read of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Previous {
    pub(crate) range: u32,
    pub(crate) through: u32,
    pub(crate) end: EntriesEnd,
    pub(crate) continues: bool,
}
