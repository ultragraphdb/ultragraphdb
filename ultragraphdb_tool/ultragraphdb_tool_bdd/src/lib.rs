/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The port's test harness: locates a crate's feature files and test data under both build systems, loads and
//! compares golden vectors generated from Dgraph's Go source, and replays the inputs fuzzing found. A test or a
//! scenario step returns [`TestResult`] and fails through `?` with a [`TestError`], never by `unwrap`, `expect` or
//! `panic!`; [`Context`] names what an absent value or a failed result should have been.

mod traits;
mod types;
mod utils;

pub use crate::traits::context::Context;
pub use crate::types::golden::Golden;
pub use crate::types::test_error::{TestError, TestResult};
pub use crate::utils::compare::compare_bytes;
pub use crate::utils::fuzz_regressions::{fuzz_regressions, fuzz_regressions_in};
pub use crate::utils::golden_io::load_golden;
pub use crate::utils::workspace::{features_dir, testdata_dir};
