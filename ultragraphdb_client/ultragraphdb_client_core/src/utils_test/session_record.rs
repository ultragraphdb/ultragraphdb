/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::OperationResult;

/// A session's entry in the test server's table: its last committed request and that reply's commit version and
/// operation results, which a retry of the request is answered with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SessionRecord {
    pub last: u64,
    pub version: u64,
    pub results: Vec<OperationResult>,
}
