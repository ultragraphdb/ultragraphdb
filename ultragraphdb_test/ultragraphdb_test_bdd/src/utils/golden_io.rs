/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fs;
use std::path::Path;

use serde::de::DeserializeOwned;

use crate::types::golden::Golden;

/// Loads a golden-vector file.
pub fn load_golden<T: DeserializeOwned>(path: &Path) -> Golden<T> {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read golden file {}: {e}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("cannot parse golden file {}: {e}", path.display()))
}
