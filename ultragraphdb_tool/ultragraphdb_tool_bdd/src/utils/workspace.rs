/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::env;
use std::path::PathBuf;

/// The running test's crate's `features/` directory: under Bazel in the runfiles tree, under Cargo beside the manifest.
/// Read from the environment at runtime, never through `env!`, which rules_rs refuses to embed.
///
/// # Panics
///
/// Panics when Bazel's `TEST_SRCDIR`, `TEST_WORKSPACE` and `TEST_TARGET` are not all set as Unicode and Cargo's
/// `CARGO_MANIFEST_DIR` is unset or not Unicode, with `CARGO_MANIFEST_DIR is set by cargo for tests: <VarError>`, e.g.
/// `NotPresent`.
pub fn features_dir() -> PathBuf {
    package_dir().join("features")
}

/// The running test's crate's `testdata/` directory, found as `features_dir` finds `features/`.
///
/// # Panics
///
/// Panics when Bazel's `TEST_SRCDIR`, `TEST_WORKSPACE` and `TEST_TARGET` are not all set as Unicode and Cargo's
/// `CARGO_MANIFEST_DIR` is unset or not Unicode, with `CARGO_MANIFEST_DIR is set by cargo for tests: <VarError>`, e.g.
/// `NotPresent`.
pub fn testdata_dir() -> PathBuf {
    package_dir().join("testdata")
}

/// The test's package directory. Bazel names the running target in `TEST_TARGET` (`//<package>:<name>`), and the
/// package's files sit at that path in the runfiles tree; Cargo sets `CARGO_MANIFEST_DIR`.
///
/// # Panics
///
/// Panics when Bazel's `TEST_SRCDIR`, `TEST_WORKSPACE` and `TEST_TARGET` are not all set as Unicode and Cargo's
/// `CARGO_MANIFEST_DIR` is unset or not Unicode, with `CARGO_MANIFEST_DIR is set by cargo for tests: <VarError>`, e.g.
/// `NotPresent`.
fn package_dir() -> PathBuf {
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
            PathBuf::from(srcdir).join(ws).join(package)
        }
        _ => PathBuf::from(
            env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by cargo for tests"),
        ),
    }
}
