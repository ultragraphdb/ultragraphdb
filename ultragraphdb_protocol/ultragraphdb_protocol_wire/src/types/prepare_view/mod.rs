/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod prepare_view_body_view;

use crate::types::column_type::ColumnType;
use crate::types::quad_pattern::QuadPattern;

/// The body of a received Prepare frame, read in place; see [`crate::Prepare`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrepareView<'a> {
    namespace: u64,
    statement: u64,
    schema: u64,
    pattern: QuadPattern<&'a str>,
    parameter_types: Vec<ColumnType>,
    result_types: Vec<ColumnType>,
}

impl<'a> PrepareView<'a> {
    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    pub fn statement(&self) -> u64 {
        self.statement
    }

    pub fn schema(&self) -> u64 {
        self.schema
    }

    pub fn pattern(&self) -> &QuadPattern<&'a str> {
        &self.pattern
    }

    pub fn parameter_types(&self) -> &[ColumnType] {
        &self.parameter_types
    }

    pub fn result_types(&self) -> &[ColumnType] {
        &self.result_types
    }
}
