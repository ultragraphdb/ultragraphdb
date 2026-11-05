/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_partition::BoundKey;

/// The transaction a Scan names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScanBound {
    pub identity: [u8; 16],
    /// The connection and stream that bound it, which a later Bind of the same identity would not share.
    pub key: BoundKey,
}
