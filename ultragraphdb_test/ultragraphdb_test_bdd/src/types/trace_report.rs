/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

use crate::types::feature_scenario::FeatureScenario;
use crate::types::spec_scenario::SpecScenario;

/// The outcome of tracing spec scenarios to feature scenarios.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TraceReport {
    missing: Vec<SpecScenario>,
    orphans: Vec<FeatureScenario>,
    duplicates: Vec<(SpecScenario, SpecScenario)>,
    pending: BTreeMap<String, Vec<SpecScenario>>,
}

impl TraceReport {
    pub fn new(
        missing: Vec<SpecScenario>,
        orphans: Vec<FeatureScenario>,
        duplicates: Vec<(SpecScenario, SpecScenario)>,
        pending: BTreeMap<String, Vec<SpecScenario>>,
    ) -> Self {
        Self {
            missing,
            orphans,
            duplicates,
            pending,
        }
    }

    /// Archived spec scenarios that no feature names.
    pub fn missing(&self) -> &[SpecScenario] {
        &self.missing
    }

    /// Feature scenarios that no spec declares.
    pub fn orphans(&self) -> &[FeatureScenario] {
        &self.orphans
    }

    /// Pairs of spec scenarios sharing one name.
    pub fn duplicates(&self) -> &[(SpecScenario, SpecScenario)] {
        &self.duplicates
    }

    /// Scenarios of changes not yet archived that no feature names, by change.
    pub fn pending(&self) -> &BTreeMap<String, Vec<SpecScenario>> {
        &self.pending
    }

    /// Whether the trace passes: nothing missing, no orphans, no duplicates. Pending scenarios do not fail it.
    pub fn passed(&self) -> bool {
        self.missing.is_empty() && self.orphans.is_empty() && self.duplicates.is_empty()
    }
}

impl std::fmt::Display for TraceReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for s in &self.missing {
            writeln!(
                f,
                "missing feature: \"{}\" ({})",
                s.name(),
                s.file().display()
            )?;
        }
        for s in &self.orphans {
            writeln!(
                f,
                "no spec declares: \"{}\" ({})",
                s.name(),
                s.file().display()
            )?;
        }
        for (a, b) in &self.duplicates {
            writeln!(
                f,
                "duplicate scenario name: \"{}\" ({} and {})",
                a.name(),
                a.file().display(),
                b.file().display()
            )?;
        }
        for (change, scenarios) in &self.pending {
            writeln!(f, "pending in {change}: {} scenarios", scenarios.len())?;
        }
        Ok(())
    }
}
