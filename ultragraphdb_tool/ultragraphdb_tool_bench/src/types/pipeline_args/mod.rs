/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::{Path, PathBuf};

use crate::errors::bench_error::BenchError;
use crate::types::command::{dir, flags, number};

const USAGE: &str = "wal_bench --dir <path> --rate <commits/s> [options]";
const FLAGS: [&str; 6] = [
    "--dir",
    "--rate",
    "--relations",
    "--seconds",
    "--cache-mb",
    "--plp",
];

/// A pipeline run of `wal_bench`: the run's directory, which holds the store and the write-ahead log, the offered
/// rate, the records per commit, the run's length, the posting cache's share of Dgraph's `--cache size-mb` (40 %),
/// and whether the log's drive has power-loss protection, which sets the round bounds' defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineArgs {
    dir: PathBuf,
    rate: u64,
    relations: usize,
    seconds: u64,
    cache_mb: u64,
    power_loss_protection: bool,
}

impl PipelineArgs {
    pub fn new(
        dir: PathBuf,
        rate: u64,
        relations: usize,
        seconds: u64,
        cache_mb: u64,
        power_loss_protection: bool,
    ) -> Self {
        Self {
            dir,
            rate,
            relations,
            seconds,
            cache_mb,
            power_loss_protection,
        }
    }

    /// The run of `args`, the program's name left out: `--dir` and `--rate` required, `--relations` 1, `--seconds`
    /// 30, `--cache-mb` 128 and `--plp off` by default, as `store_bench apply`'s defaults and the log's.
    ///
    /// # Errors
    ///
    /// [`BenchError::Usage`] naming what cannot run.
    pub fn parse(args: &[String]) -> Result<Self, BenchError> {
        if args.is_empty() {
            return Err(BenchError::Usage(USAGE.to_string()));
        }
        let flags = flags(args, &FLAGS)?;
        let plp = match flags.get("--plp").copied().unwrap_or("off") {
            "on" => true,
            "off" => false,
            other => {
                return Err(BenchError::Usage(format!(
                    "--plp is on or off, not {other}"
                )));
            }
        };
        Ok(Self::new(
            dir(&flags)?,
            number(&flags, "--rate", None, 1)?,
            usize::try_from(number(&flags, "--relations", Some(1), 1)?).unwrap_or(usize::MAX),
            number(&flags, "--seconds", Some(30), 1)?,
            number(&flags, "--cache-mb", Some(128), 0)?,
            plp,
        ))
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

    pub fn cache_mb(&self) -> u64 {
        self.cache_mb
    }

    pub fn power_loss_protection(&self) -> bool {
        self.power_loss_protection
    }
}
