/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod execute_body_builder;

use crate::types::term_table::TermTable;

/// A prepared statement executed on a stream of its own, in the transaction it names, as a Read is: its parameters
/// bound by position, at most `limit` rows (0 for no limit), from the resume key a previous result of the same request
/// ended with, and whether each row's statement ID is returned.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `statement` | u64 |
/// | 8 | `transaction` | u64: the stream number of the transaction's Begin |
/// | 16 | `limit` | u32 |
/// | 20 | `flags` | u32: bit 0, return each row's statement ID; bits 1 to 31 zero |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the term table's records |
/// | 1 | the term table's text |
/// | 2 | the parameters, u32 term indices in order |
/// | 3 | the resume key |
///
/// The request's terms are a [`crate::TermTable`] in two sections, its records and its text; every term the body
/// names is an index into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Execute {
    statement: u64,
    transaction: u64,
    limit: u32,
    statement_ids: bool,
    terms: TermTable,
    parameters: Vec<u32>,
    resume: Vec<u8>,
}

impl Execute {
    pub fn new(
        statement: u64,
        transaction: u64,
        limit: u32,
        terms: TermTable,
        parameters: Vec<u32>,
        resume: Vec<u8>,
    ) -> Self {
        Self {
            statement,
            transaction,
            limit,
            statement_ids: false,
            terms,
            parameters,
            resume,
        }
    }

    /// The execution asking for each row's statement ID.
    pub fn with_statement_ids(mut self) -> Self {
        self.statement_ids = true;
        self
    }

    /// The stream number of the Begin of the transaction the execution runs in.
    pub fn transaction(&self) -> u64 {
        self.transaction
    }

    /// Whether each row's statement ID is returned.
    pub fn statement_ids(&self) -> bool {
        self.statement_ids
    }

    pub fn statement(&self) -> u64 {
        self.statement
    }

    pub fn limit(&self) -> u32 {
        self.limit
    }

    pub fn terms(&self) -> &TermTable {
        &self.terms
    }

    pub fn parameters(&self) -> &[u32] {
        &self.parameters
    }

    pub fn resume(&self) -> &[u8] {
        &self.resume
    }
}
