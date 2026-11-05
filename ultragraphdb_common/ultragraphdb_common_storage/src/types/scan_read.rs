/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::Keyspace;
use ultragraphdb_store_posting::PostingIdentity;

/// One range of a Scan, its keys copied out of the frame, so no receive buffer stays held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScanRead {
    pub keyspace: Keyspace,
    pub tenant: u64,
    pub first: Vec<u8>,
    /// The key past its last; `None` for the end of its tenant in its keyspace.
    pub past: Option<Vec<u8>>,
    /// Where its first key's list resumes, for a range naming an `after`.
    pub after: Option<PostingIdentity>,
}
