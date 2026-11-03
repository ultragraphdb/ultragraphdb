/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod prepare_view_body_view;

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::column_type::ColumnType;
use crate::types::command::Command;
use crate::types::pattern_slot::PatternSlot;
use crate::types::quad_pattern::QuadPattern;
use crate::types::term_table_view::TermTableView;

/// The body of a received Prepare frame, read in place; see [`crate::Prepare`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrepareView<'a> {
    namespace: u64,
    statement: u64,
    schema: u64,
    terms: TermTableView<'a>,
    pattern: QuadPattern,
    parameter_types: Vec<ColumnType>,
    result_types: Vec<ColumnType>,
}

impl<'a> PrepareView<'a> {
    /// The namespace.
    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    /// The statement's id; zero in a request.
    pub fn statement(&self) -> u64 {
        self.statement
    }

    /// The schema id of the namespace's catalog; zero in a request.
    pub fn schema(&self) -> u64 {
        self.schema
    }

    /// The request's term table.
    pub fn terms(&self) -> &TermTableView<'a> {
        &self.terms
    }

    /// The quad pattern.
    pub fn pattern(&self) -> &QuadPattern {
        &self.pattern
    }

    /// The parameters' column types, in order.
    pub fn parameter_types(&self) -> &[ColumnType] {
        &self.parameter_types
    }

    /// The result columns' types, one per unbound slot and then the reifier column's; none in a request.
    pub fn result_types(&self) -> &[ColumnType] {
        &self.result_types
    }

    /// The rule a server holds a received Prepare to, a request: no result types, refused as
    /// [`ProtocolError::count_mismatch`] (`"result types"`).
    pub fn check_request(&self) -> Result<(), ProtocolError> {
        self.result_types_count(0)
    }

    /// The rule a client holds a received Prepare to, an answer: one result type per unbound slot and one for the
    /// reifier column (`PROTOCOL.md` section 7.1), refused as [`ProtocolError::count_mismatch`] (`"result types"`).
    pub fn check_reply(&self) -> Result<(), ProtocolError> {
        let unbound = self
            .pattern
            .slots()
            .iter()
            .filter(|slot| matches!(slot, PatternSlot::Unbound))
            .count();
        self.result_types_count(unbound + 1)
    }

    fn result_types_count(&self, expected: usize) -> Result<(), ProtocolError> {
        if self.result_types.len() == expected {
            return Ok(());
        }
        Err(ProtocolError::count_mismatch(
            Command::Prepare.number(),
            "result types",
            self.result_types.len() as u64,
            expected as u64,
        ))
    }
}
