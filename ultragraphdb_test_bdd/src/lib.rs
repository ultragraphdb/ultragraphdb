/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The port's test harness: traces spec scenarios to feature scenarios, checks that every crate declares its
//! feature suite for Cargo and Bazel, locates feature files and the workspace under both build systems, and loads
//! and compares golden vectors generated from Dgraph's Go source.

mod types;
mod utils;

pub use crate::types::declaration_problem::DeclarationProblem;
pub use crate::types::feature_scenario::FeatureScenario;
pub use crate::types::golden::Golden;
pub use crate::types::spec_scenario::SpecScenario;
pub use crate::types::trace_report::TraceReport;
pub use crate::utils::compare::compare_bytes;
pub use crate::utils::declarations::check_declarations;
pub use crate::utils::golden_io::{golden_files, load_golden};
pub use crate::utils::scan::{feature_scenarios, spec_scenarios};
pub use crate::utils::trace::trace;
pub use crate::utils::workspace::{features_dir, testdata_dir, workspace_root};
