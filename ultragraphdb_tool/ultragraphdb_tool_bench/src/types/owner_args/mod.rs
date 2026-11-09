/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::{Path, PathBuf};

use crate::errors::bench_error::BenchError;
use crate::types::command::{dir, flags, number};

const USAGE: &str = "wal_bench --owner-checks <n> --dir <path> [--epoch-ms 2,10]";
const FLAGS: [&str; 3] = ["--owner-checks", "--dir", "--epoch-ms"];
/// The `dev` profile's epoch bounds.
const EPOCH_MS: (u64, u64) = (2, 10);

/// A run of `wal_bench` timing the frontend's owner check: how many checks, the owner's data directory, and the
/// sequencer's epoch bounds in milliseconds, at least and at most.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerArgs {
    checks: u64,
    dir: PathBuf,
    epoch_ms: (u64, u64),
}

impl OwnerArgs {
    /// The run of `args`, the program's name left out: `--owner-checks` and `--dir` required, `--epoch-ms` 2,10 by
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
        let epoch_ms = match flags.get("--epoch-ms") {
            Some(bounds) => bounds
                .split_once(',')
                .and_then(|(least, most)| Some((least.parse().ok()?, most.parse().ok()?)))
                .ok_or_else(|| {
                    BenchError::usage(format!(
                        "--epoch-ms takes two numbers, least,most, not {bounds}"
                    ))
                })?,
            None => EPOCH_MS,
        };
        Ok(Self {
            checks: number(&flags, "--owner-checks", None, 1)?,
            dir: dir(&flags)?,
            epoch_ms,
        })
    }

    /// How many owner checks the run times.
    #[must_use]
    pub fn checks(&self) -> u64 {
        self.checks
    }

    /// The owner's data directory, which the run creates.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The sequencer's epoch bounds in milliseconds: at least and at most.
    #[must_use]
    pub fn epoch_ms(&self) -> (u64, u64) {
        self.epoch_ms
    }
}
