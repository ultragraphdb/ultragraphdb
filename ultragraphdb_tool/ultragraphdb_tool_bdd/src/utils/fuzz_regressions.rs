/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fs;
use std::path::Path;

use crate::types::test_error::{TestError, TestResult};
use crate::utils::workspace::testdata_dir;

/// The minimized inputs that once made the fuzz target `target` fail, committed under the running test's crate's
/// `testdata/fuzz_regressions/<target>/`: each file's name and bytes, in name order, so a unit test replays every finding
/// under Cargo and Bazel. A target with no such directory has none.
///
/// # Errors
///
/// As [`crate::testdata_dir`], and a [`TestError`] when a regression file or its directory cannot be read, naming it.
pub fn fuzz_regressions(target: &str) -> TestResult<Vec<(String, Vec<u8>)>> {
    fuzz_regressions_in(&testdata_dir()?.join("fuzz_regressions").join(target))
}

/// The regression cases in `dir`, as [`fuzz_regressions`] returns them: each regular file's name and bytes, in name
/// order; subdirectories are not cases. A `dir` that is absent or not a directory holds none.
///
/// # Errors
///
/// A [`TestError`] when `dir` is a directory that cannot be listed, or a regression file cannot be read, naming it.
pub fn fuzz_regressions_in(dir: &Path) -> TestResult<Vec<(String, Vec<u8>)>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let unlisted = |e: std::io::Error| {
        TestError::new(format!(
            "cannot list fuzz regressions in {}: {e}",
            dir.display()
        ))
    };
    let mut cases = Vec::new();
    for entry in fs::read_dir(dir).map_err(unlisted)? {
        let path = entry.map_err(unlisted)?.path();
        if !path.is_file() {
            continue;
        }
        let bytes = fs::read(&path).map_err(|e| {
            TestError::new(format!(
                "cannot read fuzz regression {}: {e}",
                path.display()
            ))
        })?;
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        cases.push((name, bytes));
    }
    cases.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(cases)
}
