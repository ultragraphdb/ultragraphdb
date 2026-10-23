/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::PathBuf;

/// A crate or script that does not declare its feature suite as the harness requires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclarationProblem {
    file: PathBuf,
    problem: String,
}

impl DeclarationProblem {
    pub fn new(file: impl Into<PathBuf>, problem: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            problem: problem.into(),
        }
    }

    pub fn file(&self) -> &PathBuf {
        &self.file
    }

    pub fn problem(&self) -> &str {
        &self.problem
    }
}

impl std::fmt::Display for DeclarationProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.file.display(), self.problem)
    }
}
