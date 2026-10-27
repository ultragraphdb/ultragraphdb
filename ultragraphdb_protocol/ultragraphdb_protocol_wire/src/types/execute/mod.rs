/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod execute_body_builder;

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
/// | 0 | parameters: string ends |
/// | 1 | parameters: N-Triples text |
/// | 2 | the resume key |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Execute {
    statement: u64,
    limit: u32,
    parameters: Vec<String>,
    resume: Vec<u8>,
}

impl Execute {
    pub fn new(statement: u64, limit: u32, parameters: Vec<String>, resume: Vec<u8>) -> Self {
        Self {
            statement,
            limit,
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

    pub fn parameters(&self) -> &[String] {
        &self.parameters
    }

    pub fn resume(&self) -> &[u8] {
        &self.resume
    }
}
