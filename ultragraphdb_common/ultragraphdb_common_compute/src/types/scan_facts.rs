/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::EntriesEnd;

/// What one Entries frame said of its Scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScanFacts {
    pub(crate) end: EntriesEnd,
    pub(crate) epoch: u64,
    pub(crate) version: u64,
    pub(crate) last: bool,
    pub(crate) continues: bool,
}
