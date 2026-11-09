/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::{Path, PathBuf};

use crate::errors::bench_error::BenchError;
use crate::types::command::{dir, flags, number};

const USAGE: &str = "wal_bench --inbound <reads> --partitions <n> --dir <path> [--degrees 1,20,1000] [--cache-mb 128]";
const FLAGS: [&str; 5] = [
    "--inbound",
    "--partitions",
    "--dir",
    "--degrees",
    "--cache-mb",
];
/// The in-degrees a run probes unless `--degrees` names others: P24's estimate names 20, and 1 and 1,000 bound it.
const DEGREES: [usize; 3] = [1, 20, 1_000];
/// How many in-degrees a run probes at most.
const DEGREES_MAX: usize = 8;
/// The largest in-degree a run probes: each edge is a commit of its own.
const DEGREE_MAX: u64 = 100_000;

/// A run of `wal_bench` probing an inbound point read (P24's accepted cost): how many timed reads of each object, how
/// many partitions, the run's directory, the in-degrees, each an object with that many subjects pointing at it, and the
/// cache share of each store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundArgs {
    reads: u64,
    partitions: u32,
    dir: PathBuf,
    degrees: Vec<usize>,
    cache_mb: u64,
}

impl InboundArgs {
    /// The run of `args`, the program's name left out: `--inbound`, `--partitions` and `--dir` required, `--degrees`
    /// 1,20,1000 and `--cache-mb` 128 by default.
    ///
    /// # Errors
    ///
    /// [`BenchError::usage`] naming what cannot run.
    pub fn parse(args: &[String]) -> Result<Self, BenchError> {
        if args.is_empty() {
            return Err(BenchError::usage(USAGE.to_string()));
        }
        let flags = flags(args, &FLAGS)?;
        let partitions = number(&flags, "--partitions", None, 1)?;
        let degrees = match flags.get("--degrees") {
            Some(listed) => degrees(listed)?,
            None => DEGREES.to_vec(),
        };
        Ok(Self {
            reads: number(&flags, "--inbound", None, 1)?,
            partitions: u32::try_from(partitions)
                .map_err(|_| BenchError::usage(format!("--partitions {partitions} is too many")))?,
            dir: dir(&flags)?,
            degrees,
            cache_mb: number(&flags, "--cache-mb", Some(128), 0)?,
        })
    }

    /// How many timed reads of each object the run makes.
    #[must_use]
    pub fn reads(&self) -> u64 {
        self.reads
    }

    /// How many partitions the run hosts.
    #[must_use]
    pub fn partitions(&self) -> u32 {
        self.partitions
    }

    /// The run's directory.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The in-degrees the run probes, in the order given.
    #[must_use]
    pub fn degrees(&self) -> &[usize] {
        &self.degrees
    }

    /// Each store's posting cache's size in MiB.
    #[must_use]
    pub fn cache_mb(&self) -> u64 {
        self.cache_mb
    }
}

/// The in-degrees `listed` names, comma-separated: one to [`DEGREES_MAX`] of them, each 1 to [`DEGREE_MAX`].
fn degrees(listed: &str) -> Result<Vec<usize>, BenchError> {
    let parsed = listed
        .split(',')
        .take(DEGREES_MAX + 1)
        .map(|one| {
            one.parse::<u64>()
                .ok()
                .filter(|degree| (1..=DEGREE_MAX).contains(degree))
                .and_then(|degree| usize::try_from(degree).ok())
                .ok_or_else(|| {
                    BenchError::usage(format!(
                        "--degrees takes in-degrees of 1 to {DEGREE_MAX}, not {one}"
                    ))
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if parsed.len() > DEGREES_MAX {
        return Err(BenchError::usage(format!(
            "--degrees takes at most {DEGREES_MAX} in-degrees"
        )));
    }
    Ok(parsed)
}
