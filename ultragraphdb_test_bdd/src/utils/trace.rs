/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::types::feature_scenario::FeatureScenario;
use crate::types::spec_scenario::SpecScenario;
use crate::types::trace_report::TraceReport;

/// Traces spec scenarios to feature scenarios by name.
pub fn trace(specs: &[SpecScenario], features: &[FeatureScenario]) -> TraceReport {
    let featured: HashSet<&str> = features.iter().map(|f| f.name()).collect();
    let declared: HashSet<&str> = specs.iter().map(|s| s.name()).collect();

    let mut seen: HashMap<&str, &SpecScenario> = HashMap::new();
    let mut duplicates = Vec::new();
    for s in specs {
        if let Some(first) = seen.insert(s.name(), s) {
            duplicates.push((first.clone(), s.clone()));
        }
    }

    let mut missing = Vec::new();
    let mut pending: BTreeMap<String, Vec<SpecScenario>> = BTreeMap::new();
    for s in specs.iter().filter(|s| !featured.contains(s.name())) {
        match s.change() {
            None => missing.push(s.clone()),
            Some(change) => pending
                .entry(change.to_string())
                .or_default()
                .push(s.clone()),
        }
    }

    let orphans = features
        .iter()
        .filter(|f| !declared.contains(f.name()))
        .cloned()
        .collect();
    TraceReport::new(missing, orphans, duplicates, pending)
}
