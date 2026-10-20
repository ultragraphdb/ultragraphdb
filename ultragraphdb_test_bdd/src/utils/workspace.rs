/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::env;
use std::path::PathBuf;

/// The workspace root of the source tree: under Cargo the parent of the crate's manifest directory; under Bazel the
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
        .parent()
        .expect("a crate sits under the workspace root")
        .to_path_buf()
}

/// A crate's `features/` directory: under Bazel in the runfiles tree, under Cargo beside the manifest. Read from the
/// environment at runtime, never through `env!`, which rules_rs refuses to embed.
pub fn features_dir(crate_dir: &str) -> PathBuf {
    package_dir(crate_dir).join("features")
}

/// A crate's `testdata/` directory, found as `features_dir` finds `features/`.
pub fn testdata_dir(crate_dir: &str) -> PathBuf {
    package_dir(crate_dir).join("testdata")
}

fn package_dir(crate_dir: &str) -> PathBuf {
    match (env::var("TEST_SRCDIR"), env::var("TEST_WORKSPACE")) {
        (Ok(srcdir), Ok(ws)) => PathBuf::from(srcdir).join(ws).join(crate_dir),
        _ => PathBuf::from(
            env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by cargo for tests"),
        ),
    }
}
