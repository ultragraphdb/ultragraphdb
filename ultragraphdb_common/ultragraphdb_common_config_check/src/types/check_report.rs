/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::defect::Defect;

/// What a check of a central file came to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CheckReport {
    pub(crate) targets: Vec<(String, Vec<String>)>,
    pub(crate) defects: Vec<Defect>,
    pub(crate) warnings: Vec<String>,
}

impl CheckReport {
    /// Each target with its instances.
    #[must_use]
    pub fn targets(&self) -> &[(String, Vec<String>)] {
        &self.targets
    }

    /// Every defect found.
    #[must_use]
    pub fn defects(&self) -> &[Defect] {
        &self.defects
    }

    /// What is probably a mistake, and fails nothing.
    #[must_use]
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Whether the settings pass.
    #[must_use]
    pub fn passes(&self) -> bool {
        self.defects.is_empty()
    }
}
