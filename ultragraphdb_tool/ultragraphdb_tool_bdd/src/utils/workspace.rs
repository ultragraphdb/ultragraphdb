/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::env;
use std::path::PathBuf;

use crate::types::test_error::{TestError, TestResult};

/// The running test's crate's `features/` directory: under Bazel in the runfiles tree, under Cargo beside the manifest.
/// Read from the environment at runtime, never through `env!`, which rules_rs refuses to embed.
///
/// # Errors
///
/// A [`TestError`] when Bazel's `TEST_SRCDIR`, `TEST_WORKSPACE` and `TEST_TARGET` are not all set as Unicode and
/// Cargo's `CARGO_MANIFEST_DIR` is unset or not Unicode: `CARGO_MANIFEST_DIR is set by cargo for tests: <VarError>`,
/// e.g. `environment variable not found`.
pub fn features_dir() -> TestResult<PathBuf> {
    Ok(package_dir()?.join("features"))
}

/// The running test's crate's `testdata/` directory, found as `features_dir` finds `features/`.
///
/// # Errors
///
/// A [`TestError`] when Bazel's `TEST_SRCDIR`, `TEST_WORKSPACE` and `TEST_TARGET` are not all set as Unicode and
/// Cargo's `CARGO_MANIFEST_DIR` is unset or not Unicode: `CARGO_MANIFEST_DIR is set by cargo for tests: <VarError>`,
/// e.g. `environment variable not found`.
pub fn testdata_dir() -> TestResult<PathBuf> {
    Ok(package_dir()?.join("testdata"))
}

/// The test's package directory. Bazel names the running target in `TEST_TARGET` (`//<package>:<name>`), and the
/// package's files sit at that path in the runfiles tree; Cargo sets `CARGO_MANIFEST_DIR`.
///
/// # Errors
///
/// A [`TestError`] when Bazel's `TEST_SRCDIR`, `TEST_WORKSPACE` and `TEST_TARGET` are not all set as Unicode and
/// Cargo's `CARGO_MANIFEST_DIR` is unset or not Unicode: `CARGO_MANIFEST_DIR is set by cargo for tests: <VarError>`,
/// e.g. `environment variable not found`.
fn package_dir() -> TestResult<PathBuf> {
    match (
        env::var("TEST_SRCDIR"),
        env::var("TEST_WORKSPACE"),
        env::var("TEST_TARGET"),
    ) {
        (Ok(srcdir), Ok(ws), Ok(target)) => {
            let package = target
                .trim_start_matches('@')
                .trim_start_matches("//")
                .split(':')
                .next()
                .unwrap_or_default()
                .to_string();
            Ok(PathBuf::from(srcdir).join(ws).join(package))
        }
        _ => env::var("CARGO_MANIFEST_DIR")
            .map(PathBuf::from)
            .map_err(|e| {
                TestError::new(format!("CARGO_MANIFEST_DIR is set by cargo for tests: {e}"))
            }),
    }
}
