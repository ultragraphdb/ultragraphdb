/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::{Path, PathBuf};

use crate::errors::bench_error::BenchError;
use crate::types::command::{dir, flags, number};

const USAGE: &str = "wal_bench --partitions <n> --dir <path> --rate <commits/s> [options]";
const FLAGS: [&str; 10] = [
    "--partitions",
    "--dir",
    "--rate",
    "--relations",
    "--seconds",
    "--commits",
    "--cross-percent",
    "--seed",
    "--cache-mb",
    "--verify",
];

/// A run of `wal_bench` over partitions (partitions, "The benchmark measures N partitions in one process"): how many
/// partitions, the run's directory, each partition's offered rate, the relations per commit, the run's length or each
/// partition's commits, the share of edges pointing at other partitions' entities, the seed, the cache share of each
/// store and whether the run's reverse reads are compared with the same commits on one partition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionArgs {
    partitions: u32,
    dir: PathBuf,
    rate: u64,
    relations: usize,
    seconds: u64,
    commits: u64,
    cross_percent: u8,
    seed: u64,
    cache_mb: u64,
    verify: bool,
}

impl PartitionArgs {
    /// The run of `args`, the program's name left out: `--partitions`, `--dir` and `--rate` required, `--relations` 1,
    /// `--seconds` 30, `--commits` 0, which leaves the run to its length, `--cross-percent` 0, `--seed` 1, `--cache-mb`
    /// 128 and `--verify off` by default.
    ///
    /// # Errors
    ///
    /// [`BenchError::usage`] naming what cannot run.
    pub fn parse(args: &[String]) -> Result<Self, BenchError> {
        if args.is_empty() {
            return Err(BenchError::usage(USAGE.to_string()));
        }
        let flags = flags(args, &FLAGS)?;
        let verify = match flags.get("--verify").copied().unwrap_or("off") {
            "on" => true,
            "off" => false,
            other => {
                return Err(BenchError::usage(format!(
                    "--verify is on or off, not {other}"
                )));
            }
        };
        let cross = number(&flags, "--cross-percent", Some(0), 0)?;
        let cross_percent = u8::try_from(cross)
            .ok()
            .filter(|percent| *percent <= 100)
            .ok_or_else(|| {
                BenchError::usage(format!("--cross-percent is at most 100, not {cross}"))
            })?;
        let partitions = number(&flags, "--partitions", None, 1)?;
        Ok(Self {
            partitions: u32::try_from(partitions)
                .map_err(|_| BenchError::usage(format!("--partitions {partitions} is too many")))?,
            dir: dir(&flags)?,
            rate: number(&flags, "--rate", None, 1)?,
            relations: usize::try_from(number(&flags, "--relations", Some(1), 1)?)
                .unwrap_or(usize::MAX),
            seconds: number(&flags, "--seconds", Some(30), 1)?,
            commits: number(&flags, "--commits", Some(0), 0)?,
            cross_percent,
            seed: number(&flags, "--seed", Some(1), 0)?,
            cache_mb: number(&flags, "--cache-mb", Some(128), 0)?,
            verify,
        })
    }

    /// How many partitions the run hosts.
    pub fn partitions(&self) -> u32 {
        self.partitions
    }

    /// The run's directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Each partition's offered rate, in commits a second.
    pub fn rate(&self) -> u64 {
        self.rate
    }

    /// The relations per commit.
    pub fn relations(&self) -> usize {
        self.relations
    }

    /// The run's length in seconds, unless it runs to [`PartitionArgs::commits`].
    pub fn seconds(&self) -> u64 {
        self.seconds
    }

    /// Each partition's commits; 0 leaves the run to its length.
    pub fn commits(&self) -> u64 {
        self.commits
    }

    /// The share of edges pointing at entities other partitions own, in percent.
    pub fn cross_percent(&self) -> u8 {
        self.cross_percent
    }

    /// The seed each partition's generator is seeded from.
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Each store's posting cache's size in MiB.
    pub fn cache_mb(&self) -> u64 {
        self.cache_mb
    }

    /// Whether the run's reverse reads are compared with the same commits on one partition.
    pub fn verify(&self) -> bool {
        self.verify
    }
}
