/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::ops::RangeInclusive;

use crate::errors::fuzz_args_error::FuzzArgsError;

const DEFAULT_SEEDS: u64 = 1000;
const DEFAULT_STEPS: u64 = 2000;
/// The flag that has the workload's readers read the keyspace without versions too.
pub(crate) const VERSIONLESS_READS: &str = "--versionless-reads";

/// The run `store_sim_fuzz [--versionless-reads] [first seed] [seeds] [steps]` names: the seeds from the first to the
/// last, each run for the steps, with the readers reading the keyspace without versions when the flag is given. Every seed of the range is a `u64`, so the run covers exactly the seeds it counts and every seed,
/// `u64::MAX` included, replays as the first seed of a one-seed run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FuzzArgs {
    first: u64,
    seeds: u64,
    steps: u64,
    versionless_reads: bool,
}

impl FuzzArgs {
    /// The run of `args`, the program's name left out: the flag `--versionless-reads` first if given, then each
    /// argument a whole number in decimal digits alone at its position among the numbers, the first seed
    /// `fresh_seed` when none is given, 1000 seeds and 2000 steps by default. Off by default, the flag leaves every
    /// seed's run as it was.
    ///
    /// # Errors
    ///
    /// [`FuzzArgsError::UnknownFlag`] for a first argument starting with `--` other than the flag; then the leftmost
    /// number refused, as [`FuzzArgsError::NotANumber`] or [`FuzzArgsError::TooMany`] past the third; then
    /// [`FuzzArgsError::NoSeeds`] for a count of 0, and [`FuzzArgsError::PastLastSeed`] for a range running past
    /// `u64::MAX`.
    pub fn parse(args: &[String], fresh_seed: u64) -> Result<Self, FuzzArgsError> {
        let (versionless_reads, args) = match args.split_first() {
            Some((flag, rest)) if flag == VERSIONLESS_READS => (true, rest),
            Some((flag, _)) if flag.starts_with("--") => {
                return Err(FuzzArgsError::UnknownFlag(flag.clone()));
            }
            _ => (false, args),
        };
        let mut numbers = [None; 3];
        for (index, arg) in args.iter().enumerate() {
            let slot = numbers
                .get_mut(index)
                .ok_or_else(|| FuzzArgsError::TooMany(index + 1, arg.clone()))?;
            // `u64`'s parse takes a leading '+', and Go's strconv.ParseUint does not: each number has one spelling.
            *slot = Some(
                arg.parse::<u64>()
                    .ok()
                    .filter(|_| !arg.starts_with('+'))
                    .ok_or_else(|| FuzzArgsError::NotANumber(index + 1, arg.clone()))?,
            );
        }
        let [first, seeds, steps] = numbers;
        let first = first.unwrap_or(fresh_seed);
        let seeds = seeds.unwrap_or(DEFAULT_SEEDS);
        if seeds == 0 {
            return Err(FuzzArgsError::NoSeeds());
        }
        if seeds - 1 > u64::MAX - first {
            return Err(FuzzArgsError::PastLastSeed(first, seeds));
        }
        Ok(Self {
            first,
            seeds,
            steps: steps.unwrap_or(DEFAULT_STEPS),
            versionless_reads,
        })
    }

    /// Whether the readers read the keyspace without versions too.
    pub fn versionless_reads(&self) -> bool {
        self.versionless_reads
    }

    pub fn first(&self) -> u64 {
        self.first
    }

    pub fn seeds(&self) -> u64 {
        self.seeds
    }

    pub fn steps(&self) -> u64 {
        self.steps
    }

    /// The last seed the run runs; [`FuzzArgs::parse`] holds it within `u64`.
    pub fn last(&self) -> u64 {
        self.first.saturating_add(self.seeds.saturating_sub(1))
    }

    /// The seeds the run runs, in order.
    pub fn seeds_run(&self) -> RangeInclusive<u64> {
        self.first..=self.last()
    }
}
