/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::Keyspace;

/// One range of a Scan, its keys copied out of the frame, so no receive buffer stays held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScanRead {
    pub keyspace: Keyspace,
    pub namespace: u64,
    pub first: Vec<u8>,
    /// The key past its last; `None` for the end of its namespace in its keyspace.
    pub past: Option<Vec<u8>>,
}
