/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod execute_body_builder;

use crate::types::term_table::TermTable;

/// A prepared statement executed with its parameters bound by position, at most `limit` rows (0 for no limit), from
/// the resume key a previous result ended with.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `statement` | u64 |
/// | 8 | `limit` | u32 |
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
    limit: u32,
    terms: TermTable,
    parameters: Vec<u32>,
    resume: Vec<u8>,
}

impl Execute {
    pub fn new(
        statement: u64,
        limit: u32,
        terms: TermTable,
        parameters: Vec<u32>,
        resume: Vec<u8>,
    ) -> Self {
        Self {
            statement,
            limit,
            terms,
            parameters,
            resume,
        }
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
