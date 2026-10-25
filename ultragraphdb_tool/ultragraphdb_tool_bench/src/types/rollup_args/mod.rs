/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::{Path, PathBuf};

/// A rollup run: the store's directory, the hubs whose reverse lists make the large predicate, the versions each
/// list is given, and the records per loading commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollupArgs {
    dir: PathBuf,
    hubs: u64,
    versions: u64,
    relations: usize,
}

impl RollupArgs {
    pub fn new(dir: PathBuf, hubs: u64, versions: u64, relations: usize) -> Self {
        Self {
            dir,
            hubs,
            versions,
            relations,
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn hubs(&self) -> u64 {
        self.hubs
    }

    pub fn versions(&self) -> u64 {
        self.versions
    }

    pub fn relations(&self) -> usize {
        self.relations
    }
}
