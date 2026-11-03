/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod execute_body_builder;

use crate::types::term_table::TermTable;

/// A prepared statement executed on a stream of its own, in the transaction it names, as a Read is: its parameters
/// bound by position, at most `limit` rows (0 for no limit), from the resume key a previous result of the same request
/// ended with.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `statement` | u64 |
/// | 8 | `transaction` | u64: the stream number of the transaction's Begin |
/// | 16 | `limit` | u32 |
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
    terms: TermTable,
    parameters: Vec<u32>,
    resume: Vec<u8>,
}

impl Execute {
    /// An execution of `statement` in `transaction`, binding `parameters` from `terms`, at most `limit` rows from `resume`.
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
            terms,
            parameters,
            resume,
        }
    }

    /// The stream number of the Begin of the transaction the execution runs in.
    pub fn transaction(&self) -> u64 {
        self.transaction
    }

    /// The prepared statement's id.
    pub fn statement(&self) -> u64 {
        self.statement
    }

    /// The most rows returned; 0 for no limit.
    pub fn limit(&self) -> u32 {
        self.limit
    }

    /// The request's term table.
    pub fn terms(&self) -> &TermTable {
        &self.terms
    }

    /// The parameters, term indices in order.
    pub fn parameters(&self) -> &[u32] {
        &self.parameters
    }

    /// The resume key: where a previous result of the same request ended; empty to start.
    pub fn resume(&self) -> &[u8] {
        &self.resume
    }
}
