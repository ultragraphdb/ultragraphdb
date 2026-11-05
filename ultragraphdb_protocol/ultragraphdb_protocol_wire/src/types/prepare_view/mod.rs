/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod prepare_view_body_view;

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::column_type::ColumnType;
use crate::types::command::Command;
use crate::types::language::Language;
use crate::types::pattern_slot::PatternSlot;
use crate::types::quad_pattern::QuadPattern;
use crate::types::string_list::StringList;
use crate::types::term_table_view::TermTableView;

/// The body of a received Prepare frame, read in place; see [`crate::Prepare`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrepareView<'a> {
    tenant: u64,
    statement: u64,
    schema: u64,
    language: Language,
    terms: TermTableView<'a>,
    pattern: Option<QuadPattern>,
    text: &'a str,
    parameter_types: Vec<ColumnType>,
    result_types: Vec<ColumnType>,
    names: StringList<'a>,
}

impl<'a> PrepareView<'a> {
    /// The tenant.
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The statement's id; zero in a request.
    pub fn statement(&self) -> u64 {
        self.statement
    }

    /// The schema id of the tenant's catalog; zero in a request.
    pub fn schema(&self) -> u64 {
        self.schema
    }

    /// What the statement is written in.
    pub fn language(&self) -> Language {
        self.language
    }

    /// The request's term table.
    pub fn terms(&self) -> &TermTableView<'a> {
        &self.terms
    }

    /// The quad pattern; `None` for a text.
    pub fn pattern(&self) -> Option<&QuadPattern> {
        self.pattern.as_ref()
    }

    /// The query text, borrowed from the frame; empty for a pattern.
    pub fn text(&self) -> &'a str {
        self.text
    }

    /// The parameters' column types, in order.
    pub fn parameter_types(&self) -> &[ColumnType] {
        &self.parameter_types
    }

    /// The result columns' types: one per unbound slot and then the reifier column's, or one per projection; none in a
    /// request.
    pub fn result_types(&self) -> &[ColumnType] {
        &self.result_types
    }

    /// For a text, the parameter names, then in a reply the result column names; empty for a pattern.
    pub fn names(&self) -> StringList<'a> {
        self.names
    }

    /// The rule a server holds a received Prepare to, a request: no result types, refused as
    /// [`ProtocolError::count_mismatch`] (`"result types"`).
    pub fn check_request(&self) -> Result<(), ProtocolError> {
        self.result_types_count(0)
    }

    /// The rule a client holds a received Prepare to, an answer: one result type per unbound slot and one for the
    /// reifier column (`PROTOCOL.md` §8.1), refused as [`ProtocolError::count_mismatch`] (`"result types"`). A text's, whose
    /// statement may project nothing, its view's names already count.
    pub fn check_reply(&self) -> Result<(), ProtocolError> {
        let expected = match &self.pattern {
            Some(pattern) => {
                pattern
                    .slots()
                    .iter()
                    .filter(|slot| matches!(slot, PatternSlot::Unbound))
                    .count()
                    + 1
            }
            None => self.result_types.len(),
        };
        self.result_types_count(expected)
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
