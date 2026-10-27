/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fs;
use std::path::Path;

use serde::de::DeserializeOwned;

use crate::types::golden::Golden;

/// Loads a golden-vector file.
///
/// # Panics
///
/// Panics when the file cannot be read, with `cannot read golden file <path>: <io error>`, and when its text is not a
/// golden file of `T` cases, with `cannot parse golden file <path>: <serde_json error>`. A test fails on either, naming
/// the file and the fault.
pub fn load_golden<T: DeserializeOwned>(path: &Path) -> Golden<T> {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read golden file {}: {e}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("cannot parse golden file {}: {e}", path.display()))
}
