/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod commit_body_builder;

use crate::types::operation_result::OperationResult;

/// A transaction commits, with its session and request number (D9): sent with `commit_version` zero and no results,
/// answered with its commit version and the result of each commit-time operation it asked for, in the order asked.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `session` | u64 |
/// | 8 | `request` | u64 |
/// | 16 | `commit_version` | u64 |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | results, 16 bytes each: `uid` u64, `created` u8 (0 or 1), 7 zero bytes |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    session: u64,
    request: u64,
    commit_version: u64,
    results: Vec<OperationResult>,
}

impl Commit {
    pub fn new(
        session: u64,
        request: u64,
        commit_version: u64,
        results: Vec<OperationResult>,
    ) -> Self {
        Self {
            session,
            request,
            commit_version,
            results,
        }
    }

    pub fn session(&self) -> u64 {
        self.session
    }

    pub fn request(&self) -> u64 {
        self.request
    }

    pub fn commit_version(&self) -> u64 {
        self.commit_version
    }

    pub fn results(&self) -> &[OperationResult] {
        &self.results
    }
}
