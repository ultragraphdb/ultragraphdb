/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::contents::Contents;

/// What a run that broke no invariant did: its trace, every step's choice and every operation and fault at the
/// store, and what the store held at its end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutcome {
    trace: Vec<String>,
    contents: Contents,
}

impl RunOutcome {
    pub(crate) fn new(trace: Vec<String>, contents: Contents) -> Self {
        Self { trace, contents }
    }

    pub fn trace(&self) -> &[String] {
        &self.trace
    }

    pub fn contents(&self) -> &Contents {
        &self.contents
    }
}
