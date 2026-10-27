/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod prepare_body_builder;

use crate::types::column_type::ColumnType;
use crate::types::quad_pattern::QuadPattern;
use crate::types::term_table::TermTable;

/// A statement prepared once per session (stage 5: a quad pattern with parameter slots): sent with the pattern and its
/// parameters' types, `statement` and `schema` zero and no result types; answered with the statement's id, the
/// schema id of its namespace's catalog, and its parameter and result column types.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `namespace` | u64 |
/// | 8 | `statement` | u64 |
/// | 16 | `schema` | u64 |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the pattern: four [`crate::PatternSlot`]s |
/// | 1 | the term table's records |
/// | 2 | the term table's text |
/// | 3 | parameter types, one [`crate::ColumnType`] byte per parameter |
/// | 4 | result column types, one byte per unbound slot, or none in a request |
///
/// The request's terms are a [`crate::TermTable`] in two sections, its records and its text; every term the body
/// names is an index into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepare {
    namespace: u64,
    statement: u64,
    schema: u64,
    terms: TermTable,
    pattern: QuadPattern,
    parameter_types: Vec<ColumnType>,
    result_types: Vec<ColumnType>,
}

impl Prepare {
    pub fn new(
        namespace: u64,
        statement: u64,
        schema: u64,
        terms: TermTable,
        pattern: QuadPattern,
        parameter_types: Vec<ColumnType>,
        result_types: Vec<ColumnType>,
    ) -> Self {
        Self {
            namespace,
            statement,
            schema,
            terms,
            pattern,
            parameter_types,
            result_types,
        }
    }

    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    pub fn statement(&self) -> u64 {
        self.statement
    }

    pub fn schema(&self) -> u64 {
        self.schema
    }

    pub fn terms(&self) -> &TermTable {
        &self.terms
    }

    pub fn pattern(&self) -> &QuadPattern {
        &self.pattern
    }

    pub fn parameter_types(&self) -> &[ColumnType] {
        &self.parameter_types
    }

    pub fn result_types(&self) -> &[ColumnType] {
        &self.result_types
    }
}
