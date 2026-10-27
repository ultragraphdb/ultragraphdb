/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fs;
use std::path::Path;

use crate::utils::workspace::testdata_dir;

/// The minimized inputs that once made the fuzz target `target` fail, committed under the running test's crate's
/// `testdata/fuzz_regressions/<target>/`: each file's name and bytes, in name order, so a unit test replays every finding
/// under Cargo and Bazel. A target with no such directory has none.
///
/// # Panics
///
/// Panics as [`crate::testdata_dir`] does, and when a regression file or its directory cannot be read, naming it.
pub fn fuzz_regressions(target: &str) -> Vec<(String, Vec<u8>)> {
    fuzz_regressions_in(&testdata_dir().join("fuzz_regressions").join(target))
}

/// The regression cases in `dir`, as [`fuzz_regressions`] returns them: each regular file's name and bytes, in name
/// order; subdirectories are not cases. A `dir` that is absent or not a directory holds none.
///
/// # Panics
///
/// Panics when `dir` is a directory that cannot be listed, or a regression file cannot be read, naming it.
pub fn fuzz_regressions_in(dir: &Path) -> Vec<(String, Vec<u8>)> {
    if !dir.is_dir() {
        return Vec::new();
    }
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot list fuzz regressions in {}: {e}", dir.display()));
    let mut cases: Vec<(String, Vec<u8>)> = entries
        .map(|entry| {
            entry.unwrap_or_else(|e| {
                panic!("cannot list fuzz regressions in {}: {e}", dir.display())
            })
        })
        .filter(|entry| entry.path().is_file())
        .map(|entry| {
            let path = entry.path();
            let bytes = fs::read(&path)
                .unwrap_or_else(|e| panic!("cannot read fuzz regression {}: {e}", path.display()));
            (entry.file_name().to_string_lossy().into_owned(), bytes)
        })
        .collect();
    cases.sort_by(|a, b| a.0.cmp(&b.0));
    cases
}
