/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A partition's report on a catalog version (`PROTOCOL.md` §13.19), its terms held past its frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportAsked {
    /// The tenant.
    pub tenant: u64,
    /// The version reported on.
    pub version: u64,
    /// The reporting partition.
    pub partition: u32,
    /// What the data refused: the predicate and the term at fault; `None` for a check passed.
    pub refused: Option<(String, String)>,
}
