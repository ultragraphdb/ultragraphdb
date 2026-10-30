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
    statement_ids: bool,
    terms: TermTableView<'a>,
    parameters: Vec<u32>,
    resume: &'a [u8],
}

impl<'a> ExecuteView<'a> {
    pub fn statement(&self) -> u64 {
        self.statement
    }

    pub fn transaction(&self) -> u64 {
        self.transaction
    }

    pub fn statement_ids(&self) -> bool {
        self.statement_ids
    }

    pub fn limit(&self) -> u32 {
        self.limit
    }

    pub fn terms(&self) -> &TermTableView<'a> {
        &self.terms
    }

    pub fn parameters(&self) -> &[u32] {
        &self.parameters
    }

    pub fn resume(&self) -> &'a [u8] {
        self.resume
    }
}
