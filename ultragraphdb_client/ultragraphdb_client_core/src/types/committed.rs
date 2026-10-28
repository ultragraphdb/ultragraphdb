/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::OperationResult;

/// A commit's answer: its commit version and the result of each commit-time operation, in the order asked; the
/// request number of its session it committed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Committed {
    version: u64,
    request: u64,
    results: Vec<OperationResult>,
}

impl Committed {
    pub(crate) fn new(version: u64, request: u64, results: Vec<OperationResult>) -> Self {
        Self {
            version,
            request,
            results,
        }
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn request(&self) -> u64 {
        self.request
    }

    pub fn results(&self) -> &[OperationResult] {
        &self.results
    }
}
