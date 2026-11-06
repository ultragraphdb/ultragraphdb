/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SessionToken;
use ultragraphdb_protocol_wire::OperationResult;

/// What the session table holds of a session: its last committed request, and that request's version, token and
/// results, which a retry is answered from.
#[derive(Debug, Clone, Default)]
pub(crate) struct SessionRecord {
    pub last: u64,
    pub version: u64,
    pub token: Option<SessionToken>,
    pub results: Vec<OperationResult>,
}
