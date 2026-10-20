/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::PathBuf;

/// A `#### Scenario:` of a spec: archived under `openspec/specs/`, or in a change under `openspec/changes/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecScenario {
    name: String,
    file: PathBuf,
    change: Option<String>,
}

impl SpecScenario {
    pub fn new(name: impl Into<String>, file: impl Into<PathBuf>, change: Option<String>) -> Self {
        Self {
            name: name.into(),
            file: file.into(),
            change,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn file(&self) -> &PathBuf {
        &self.file
    }

    /// The change declaring this scenario; `None` for an archived spec.
    pub fn change(&self) -> Option<&str> {
        self.change.as_deref()
    }
}
