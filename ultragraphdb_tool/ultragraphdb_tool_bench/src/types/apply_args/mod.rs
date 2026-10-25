/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::{Path, PathBuf};

use ultragraphdb_store_kv::Durability;

/// An apply run: the store's directory, the offered rate, the records per commit, the run's length, the store's
/// durability, whether rollups run beside the apply thread as Dgraph's do, and the posting cache's share of Dgraph's
/// `--cache size-mb` (40 %), and the cap on an apply step's commits, if any, beside Dgraph's 64 MiB `Ready` bound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyArgs {
    dir: PathBuf,
    rate: u64,
    relations: usize,
    seconds: u64,
    durability: Durability,
    rollups: bool,
    cache_mb: u64,
    max_batch: Option<usize>,
}

impl ApplyArgs {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        dir: PathBuf,
        rate: u64,
        relations: usize,
        seconds: u64,
        durability: Durability,
        rollups: bool,
        cache_mb: u64,
        max_batch: Option<usize>,
    ) -> Self {
        Self {
            dir,
            rate,
            relations,
            seconds,
            durability,
            rollups,
            cache_mb,
            max_batch,
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn rate(&self) -> u64 {
        self.rate
    }

    pub fn relations(&self) -> usize {
        self.relations
    }

    pub fn seconds(&self) -> u64 {
        self.seconds
    }

    pub fn durability(&self) -> Durability {
        self.durability
    }

    pub fn rollups(&self) -> bool {
        self.rollups
    }

    pub fn cache_mb(&self) -> u64 {
        self.cache_mb
    }

    pub fn max_batch(&self) -> Option<usize> {
        self.max_batch
    }
}
