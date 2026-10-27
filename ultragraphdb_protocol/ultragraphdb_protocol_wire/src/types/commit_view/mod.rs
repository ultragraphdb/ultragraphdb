/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod commit_view_body_view;

use crate::types::operation_result::OperationResult;

/// The size of an operation result.
pub(crate) const RESULT_SIZE: usize = 16;

/// The body of a received Commit frame, read in place; see [`crate::Commit`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommitView<'a> {
    session: u64,
    request: u64,
    commit_version: u64,
    results: &'a [u8],
}

impl<'a> CommitView<'a> {
    pub fn session(&self) -> u64 {
        self.session
    }

    pub fn request(&self) -> u64 {
        self.request
    }

    pub fn commit_version(&self) -> u64 {
        self.commit_version
    }

    /// The number of results.
    pub fn result_count(&self) -> usize {
        self.results.len() / RESULT_SIZE
    }

    /// Result `index`, `None` past the last.
    pub fn result(&self, index: usize) -> Option<OperationResult> {
        let record = self.results.get(
            index.checked_mul(RESULT_SIZE)?..index.checked_add(1)?.checked_mul(RESULT_SIZE)?,
        )?;
        let uid = u64::from_le_bytes(record.get(..8)?.try_into().ok()?);
        Some(OperationResult::new(uid, record.get(8) == Some(&1)))
    }
}
