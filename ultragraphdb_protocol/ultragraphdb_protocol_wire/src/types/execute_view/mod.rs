/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod execute_view_body_view;

use crate::types::term_table_view::TermTableView;

/// The body of a received Execute frame, read in place; see [`crate::Execute`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecuteView<'a> {
    statement: u64,
    transaction: u64,
    limit: u32,
    terms: TermTableView<'a>,
    parameters: Vec<u32>,
    resume: &'a [u8],
}

impl<'a> ExecuteView<'a> {
    /// The prepared statement's id.
    #[must_use]
    pub fn statement(&self) -> u64 {
        self.statement
    }

    /// The stream number of the transaction's Begin.
    #[must_use]
    pub fn transaction(&self) -> u64 {
        self.transaction
    }

    /// The most rows returned; 0 for no limit.
    #[must_use]
    pub fn limit(&self) -> u32 {
        self.limit
    }

    /// The request's term table.
    #[must_use]
    pub fn terms(&self) -> &TermTableView<'a> {
        &self.terms
    }

    /// The parameters, term indices in order.
    #[must_use]
    pub fn parameters(&self) -> &[u32] {
        &self.parameters
    }

    /// The resume key: where a previous result of the same request ended; empty to start.
    #[must_use]
    pub fn resume(&self) -> &'a [u8] {
        self.resume
    }
}
