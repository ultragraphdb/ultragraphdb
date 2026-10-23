/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::types::feature_scenario::FeatureScenario;
use crate::types::spec_scenario::SpecScenario;

const SKIPPED_DIRS: [&str; 4] = ["target", "target-bzl", ".git", "node_modules"];

/// Every `#### Scenario:` under `openspec/specs/` (archived) and `openspec/changes/<change>/specs/`, excluding
/// `openspec/changes/archive/`.
pub fn spec_scenarios(root: &Path) -> io::Result<Vec<SpecScenario>> {
    let mut out = Vec::new();
    let archived = root.join("openspec").join("specs");
    for file in files_named(&archived, |p| p.file_name().is_some_and(|n| n == "spec.md"))? {
        out.extend(
            scenarios_in(&file, "#### Scenario:")?
                .into_iter()
                .map(|n| SpecScenario::new(n, file.clone(), None)),
        );
    }
    let changes = root.join("openspec").join("changes");
    if changes.is_dir() {
        let mut entries: Vec<_> = fs::read_dir(&changes)?.collect::<Result<_, _>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries
            .into_iter()
            .filter(|e| e.path().is_dir() && e.file_name() != "archive")
        {
            let change = entry.file_name().to_string_lossy().into_owned();
            for file in files_named(&entry.path().join("specs"), |p| {
                p.file_name().is_some_and(|n| n == "spec.md")
            })? {
                out.extend(
                    scenarios_in(&file, "#### Scenario:")?
                        .into_iter()
                        .map(|n| SpecScenario::new(n, file.clone(), Some(change.clone()))),
                );
            }
        }
    }
    Ok(out)
}

/// Every Gherkin `Scenario:` in a `.feature` file under `root`, skipping build output.
pub fn feature_scenarios(root: &Path) -> io::Result<Vec<FeatureScenario>> {
    let mut out = Vec::new();
    for file in files_named(root, |p| p.extension().is_some_and(|e| e == "feature"))? {
        out.extend(
            scenarios_in(&file, "Scenario:")?
                .into_iter()
                .map(|n| FeatureScenario::new(n, file.clone())),
        );
    }
    Ok(out)
}

/// Every crate directory below the root: each directory other than the root that holds a `Cargo.toml`, in path order.
pub(crate) fn crate_dirs(root: &Path) -> io::Result<Vec<PathBuf>> {
    Ok(
        files_named(root, |p| p.file_name().is_some_and(|n| n == "Cargo.toml"))?
            .into_iter()
            .filter_map(|m| m.parent().map(Path::to_path_buf))
            .filter(|d| d != root)
            .collect(),
    )
}

fn scenarios_in(file: &Path, marker: &str) -> io::Result<Vec<String>> {
    Ok(fs::read_to_string(file)?
        .lines()
        .filter_map(|l| l.trim_start().strip_prefix(marker))
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .collect())
}

fn files_named(dir: &Path, keep: impl Fn(&Path) -> bool + Copy) -> io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() {
            if !SKIPPED_DIRS.iter().any(|s| name == *s) {
                out.extend(files_named(&path, keep)?);
            }
        } else if keep(&path) {
            out.push(path);
        }
    }
    Ok(out)
}
