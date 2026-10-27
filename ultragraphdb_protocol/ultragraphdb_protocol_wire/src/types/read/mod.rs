/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod read_body_builder;

use crate::types::quad_pattern::QuadPattern;

/// A data request of stage 5 (in a transaction on the frame's stream): a quad pattern, at most `limit` rows (0 for
/// no limit), and the resume key a previous result ended with, empty to start.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `namespace` | u64 |
/// | 8 | `limit` | u32 |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the pattern: four [`crate::PatternSlot`]s, no parameter |
/// | 1 | the pattern's terms: string ends |
/// | 2 | the pattern's terms: N-Triples text |
/// | 3 | the resume key |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Read {
    namespace: u64,
    limit: u32,
    pattern: QuadPattern<String>,
    resume: Vec<u8>,
}

impl Read {
    pub fn new(namespace: u64, limit: u32, pattern: QuadPattern<String>, resume: Vec<u8>) -> Self {
        Self {
            namespace,
            limit,
            pattern,
            resume,
        }
    }

    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    pub fn limit(&self) -> u32 {
        self.limit
    }

    pub fn pattern(&self) -> &QuadPattern<String> {
        &self.pattern
    }

    pub fn resume(&self) -> &[u8] {
        &self.resume
    }
}
