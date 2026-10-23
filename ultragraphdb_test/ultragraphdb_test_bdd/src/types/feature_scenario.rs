/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::PathBuf;

/// A Gherkin `Scenario:` in a `.feature` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureScenario {
    name: String,
    file: PathBuf,
}

impl FeatureScenario {
    pub fn new(name: impl Into<String>, file: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            file: file.into(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn file(&self) -> &PathBuf {
        &self.file
    }
}
