/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod prepare_body_builder;

use crate::types::column_type::ColumnType;
use crate::types::language::Language;
use crate::types::quad_pattern::QuadPattern;
use crate::types::term_table::TermTable;

/// What a statement is prepared from: a quad pattern with parameter slots, or a query text.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Source {
    Pattern(QuadPattern),
    Text(String),
}

/// A statement prepared once per connection (`PROTOCOL.md` §8.1), a quad pattern with parameter slots or a query text of
/// the GQL or SPARQL core with named parameters: sent with its parameters' types, `statement` and `schema` zero and no
/// result types; answered with the statement's id, the schema id of its tenant's catalog, the result column types and,
/// for a text, the names.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `tenant` | u64 |
/// | 8 | `statement` | u64 |
/// | 16 | `schema` | u64 |
/// | 24 | `language` | u8, a [`Language`] |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | language 0: the pattern, four [`crate::PatternSlot`]s; 1 or 2: the query text, UTF-8 |
/// | 1 | the term table's records |
/// | 2 | the term table's text |
/// | 3 | parameter types, one [`crate::ColumnType`] byte per parameter |
/// | 4 | result column types, one byte per result column: each unbound slot, then the reifier column, or each projection; none in a request |
/// | 5 | names: ends; for a text, in a request the parameter names, in a reply the parameter names then the result column names; empty for a pattern |
/// | 6 | names: text |
///
/// The request's terms are a [`crate::TermTable`] in two sections, its records and its text; every term the body
/// names is an index into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepare {
    tenant: u64,
    statement: u64,
    schema: u64,
    language: Language,
    terms: TermTable,
    source: Source,
    parameter_types: Vec<ColumnType>,
    result_types: Vec<ColumnType>,
    names: Vec<String>,
}

impl Prepare {
    /// A Prepare of `pattern` in `tenant`, naming `terms`, with its statement, schema and column types.
    pub fn new(
        tenant: u64,
        statement: u64,
        schema: u64,
        terms: TermTable,
        pattern: QuadPattern,
        parameter_types: Vec<ColumnType>,
        result_types: Vec<ColumnType>,
    ) -> Self {
        Self {
            tenant,
            statement,
            schema,
            language: Language::Pattern,
            terms,
            source: Source::Pattern(pattern),
            parameter_types,
            result_types,
            names: Vec::new(),
        }
    }

    /// A request preparing the query `text` of `language` in `tenant`, naming `terms`, with its parameters' `names` and
    /// `parameter_types`, in parameter order.
    pub fn of_text(
        tenant: u64,
        language: Language,
        text: impl Into<String>,
        terms: TermTable,
        names: Vec<String>,
        parameter_types: Vec<ColumnType>,
    ) -> Self {
        Self {
            tenant,
            statement: 0,
            schema: 0,
            language,
            terms,
            source: Source::Text(text.into()),
            parameter_types,
            result_types: Vec::new(),
            names,
        }
    }

    /// The reply to a Prepare of a text: its `statement`, its tenant's `schema` id and each projection's name and type,
    /// the names following the parameters'.
    #[must_use]
    pub fn answered(
        mut self,
        statement: u64,
        schema: u64,
        projections: Vec<(String, ColumnType)>,
    ) -> Self {
        self.statement = statement;
        self.schema = schema;
        for (name, column_type) in projections {
            self.names.push(name);
            self.result_types.push(column_type);
        }
        self
    }

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
    #[must_use]
    pub fn language(&self) -> Language {
        self.language
    }

    /// The request's term table.
    pub fn terms(&self) -> &TermTable {
        &self.terms
    }

    /// The quad pattern; `None` for a text.
    #[must_use]
    pub fn pattern(&self) -> Option<&QuadPattern> {
        match &self.source {
            Source::Pattern(pattern) => Some(pattern),
            Source::Text(_) => None,
        }
    }

    /// The query text; `None` for a pattern.
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        match &self.source {
            Source::Pattern(_) => None,
            Source::Text(text) => Some(text),
        }
    }

    /// The parameters' column types, in order.
    pub fn parameter_types(&self) -> &[ColumnType] {
        &self.parameter_types
    }

    /// The result columns' types: one per unbound slot and the reifier column's last, or one per projection; none in a
    /// request.
    pub fn result_types(&self) -> &[ColumnType] {
        &self.result_types
    }

    /// For a text, the parameter names, then in a reply the result column names; empty for a pattern.
    #[must_use]
    pub fn names(&self) -> &[String] {
        &self.names
    }
}
