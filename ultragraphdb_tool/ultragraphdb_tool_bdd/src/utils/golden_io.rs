/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fs;
use std::path::Path;

use serde::de::DeserializeOwned;

use crate::types::golden::Golden;
use crate::types::test_error::{TestError, TestResult};

/// Loads a golden-vector file.
///
/// # Errors
///
/// A [`TestError`] when the file cannot be read, `cannot read golden file <path>: <io error>`, and when its text is
/// not a golden file of `T` cases, `cannot parse golden file <path>: <serde_json error>`. A test fails on either,
/// naming the file and the fault.
pub fn load_golden<T: DeserializeOwned>(path: &Path) -> TestResult<Golden<T>> {
    let text = fs::read_to_string(path)
        .map_err(|e| TestError::new(format!("cannot read golden file {}: {e}", path.display())))?;
    serde_json::from_str(&text)
        .map_err(|e| TestError::new(format!("cannot parse golden file {}: {e}", path.display())))
}
