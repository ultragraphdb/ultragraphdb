/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::{Path, PathBuf};

use crate::errors::bench_error::BenchError;
use crate::types::command::{dir, flags, number};

const USAGE: &str = "alloc_bench --dir <path> [--frames 10000] [--relations 1] [--hubs 1000]";
const FLAGS: [&str; 4] = ["--dir", "--frames", "--relations", "--hubs"];
/// The most relations a frame writes, as `client_bench` takes them.
const RELATIONS_MAX: u64 = 1_000;

/// A run of `alloc_bench`: its directory, how many DirectWrite frames it turns into records, the relations each frame
/// writes and the hubs their edges point at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllocArgs {
    dir: PathBuf,
    frames: u64,
    relations: usize,
    hubs: u64,
}

impl AllocArgs {
    /// The run of `args`, the program's name left out: `--dir` required.
    ///
    /// # Errors
    ///
    /// [`BenchError::usage`] naming what cannot run.
    pub fn parse(args: &[String]) -> Result<Self, BenchError> {
        if args.is_empty() {
            return Err(BenchError::usage(USAGE.to_string()));
        }
        let flags = flags(args, &FLAGS)?;
        let relations = number(&flags, "--relations", Some(1), 1)?;
        if relations > RELATIONS_MAX {
            return Err(BenchError::usage(format!(
                "--relations takes at most {RELATIONS_MAX}"
            )));
        }
        Ok(Self {
            dir: dir(&flags)?,
            frames: number(&flags, "--frames", Some(10_000), 1)?,
            // At most `RELATIONS_MAX`, so it converts.
            relations: usize::try_from(relations).map_err(|_| {
                BenchError::usage(format!("--relations takes at most {RELATIONS_MAX}"))
            })?,
            hubs: number(&flags, "--hubs", Some(1_000), 1)?,
        })
    }

    /// The run's directory, which it creates.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// How many frames the run turns into records.
    #[must_use]
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// The relations each frame writes.
    #[must_use]
    pub fn relations(&self) -> usize {
        self.relations
    }

    /// The hubs the edges point at.
    #[must_use]
    pub fn hubs(&self) -> u64 {
        self.hubs
    }
}
