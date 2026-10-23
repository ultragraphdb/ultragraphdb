/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;

use crate::types::golden::Golden;
use crate::utils::scan::crate_dirs;

/// Loads a golden-vector file.
pub fn load_golden<T: DeserializeOwned>(path: &Path) -> Golden<T> {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read golden file {}: {e}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("cannot parse golden file {}: {e}", path.display()))
}

/// Every `.json` file under a `testdata/golden/` directory of the workspace's crates, at any depth below the root.
pub fn golden_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for krate in crate_dirs(root)? {
        let dir = krate.join("testdata").join("golden");
        if dir.is_dir() {
            let mut files: Vec<_> = fs::read_dir(&dir)?.collect::<Result<_, _>>()?;
            files.sort_by_key(|e| e.file_name());
            out.extend(
                files
                    .into_iter()
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "json")),
            );
        }
    }
    Ok(out)
}
