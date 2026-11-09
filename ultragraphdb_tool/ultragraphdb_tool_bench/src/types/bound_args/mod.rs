/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::{Path, PathBuf};

use crate::errors::bench_error::BenchError;
use crate::types::command::{dir, flags, number};

const USAGE: &str = "wal_bench --bound-walks <n> --dir <path> [--repeats 1000]";
const FLAGS: [&str; 3] = ["--bound-walks", "--dir", "--repeats"];
/// The most transactions a run binds: each holds a transaction's buffers, so a million takes gigabytes.
const BOUND_MAX: u64 = 1_000_000;

/// A run of `wal_bench` timing a partition's walks of its bound transactions: how many read-write transactions the
/// partition holds open, the partition's data directory, and how many times each walk is timed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundArgs {
    bound: u64,
    dir: PathBuf,
    repeats: u64,
}

impl BoundArgs {
    /// The run of `args`, the program's name left out: `--bound-walks` and `--dir` required, `--repeats` 1,000 by
    /// default.
    ///
    /// # Errors
    ///
    /// [`BenchError::usage`] naming what cannot run.
    pub fn parse(args: &[String]) -> Result<Self, BenchError> {
        if args.is_empty() {
            return Err(BenchError::usage(USAGE.to_string()));
        }
        let flags = flags(args, &FLAGS)?;
        let bound = number(&flags, "--bound-walks", None, 0)?;
        if bound > BOUND_MAX {
            return Err(BenchError::usage(format!(
                "--bound-walks takes at most {BOUND_MAX}"
            )));
        }
        Ok(Self {
            bound,
            dir: dir(&flags)?,
            repeats: number(&flags, "--repeats", Some(1_000), 1)?,
        })
    }

    /// How many read-write transactions the partition holds open.
    #[must_use]
    pub fn bound(&self) -> u64 {
        self.bound
    }

    /// The partition's data directory, which the run creates.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// How many times each walk is timed.
    #[must_use]
    pub fn repeats(&self) -> u64 {
        self.repeats
    }
}
