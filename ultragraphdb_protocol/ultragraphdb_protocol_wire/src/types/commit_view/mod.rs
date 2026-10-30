/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod commit_view_body_view;

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::command::Command;

use crate::types::operation_result::OperationResult;
use crate::utils::results::{RESULT_SIZE, result_at};

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
        result_at(self.results, index)
    }

    /// The rule a server holds a received Commit to, a request: no results, refused as
    /// [`ProtocolError::CountMismatch`] (`"results"`).
    pub fn check_request(&self) -> Result<(), ProtocolError> {
        if self.result_count() == 0 {
            return Ok(());
        }
        Err(ProtocolError::CountMismatch(
            Command::Commit.number(),
            "results",
            self.result_count() as u64,
            0,
        ))
    }
}
