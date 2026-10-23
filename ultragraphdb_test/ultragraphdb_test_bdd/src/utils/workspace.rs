/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::env;
use std::path::PathBuf;

/// The workspace root of the source tree: under Cargo the nearest ancestor of the crate's manifest directory holding
/// `MODULE.bazel`; under Bazel the
/// directory of `MODULE.bazel`, reached through its runfiles link, which needs the `//:MODULE.bazel` data dependency
/// and a test that runs outside the sandbox (`local`).
pub fn workspace_root() -> PathBuf {
    if let (Ok(srcdir), Ok(ws)) = (env::var("TEST_SRCDIR"), env::var("TEST_WORKSPACE")) {
        let module = PathBuf::from(srcdir).join(ws).join("MODULE.bazel");
        let real = module
            .canonicalize()
            .unwrap_or_else(|e| panic!("{} must be a data dependency: {e}", module.display()));
        return real
            .parent()
            .expect("MODULE.bazel has a parent")
            .to_path_buf();
    }
    let manifest =
        env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by cargo for tests");
    PathBuf::from(manifest)
        .ancestors()
        .find(|d| d.join("MODULE.bazel").is_file())
        .expect("a crate sits below the directory of MODULE.bazel")
        .to_path_buf()
}

/// The running test's crate's `features/` directory: under Bazel in the runfiles tree, under Cargo beside the manifest.
/// Read from the environment at runtime, never through `env!`, which rules_rs refuses to embed.
pub fn features_dir() -> PathBuf {
    package_dir().join("features")
}

/// The running test's crate's `testdata/` directory, found as `features_dir` finds `features/`.
pub fn testdata_dir() -> PathBuf {
    package_dir().join("testdata")
}

/// The test's package directory. Bazel names the running target in `TEST_TARGET` (`//<package>:<name>`), and the
/// package's files sit at that path in the runfiles tree; Cargo sets `CARGO_MANIFEST_DIR`.
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
