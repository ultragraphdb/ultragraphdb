/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod fuzz_args_error_display;
mod fuzz_args_error_error;

/// Why `store_sim_fuzz` refused its arguments. A position counts the numbers from 1, the program's name and the flag
/// left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuzzArgsError(pub FuzzArgsErrorEnum);

/// Each reason `store_sim_fuzz` refuses its arguments for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FuzzArgsErrorEnum {
    /// The argument at the position is no whole number a `u64` holds, written in decimal digits alone.
    NotANumber(usize, String),
    /// The argument at the position follows the last of the three positions.
    TooMany(usize, String),
    /// The seed count is 0, so the run would run no seed.
    NoSeeds,
    /// The seed count from the first seed runs past `u64::MAX`: the first seed, then the count.
    PastLastSeed(u64, u64),
    /// The first argument starts with `--` and is not the one flag, `--versionless-reads`.
    UnknownFlag(String),
}

impl FuzzArgsError {
    /// The argument `value` at `position` is no whole number a `u64` holds.
    #[must_use]
    pub fn not_a_number(position: usize, value: String) -> Self {
        Self(FuzzArgsErrorEnum::NotANumber(position, value))
    }

    /// The argument `value` at `position` follows the last of the three positions.
    #[must_use]
    pub fn too_many(position: usize, value: String) -> Self {
        Self(FuzzArgsErrorEnum::TooMany(position, value))
    }

    /// The seed count is 0.
    #[must_use]
    pub fn no_seeds() -> Self {
        Self(FuzzArgsErrorEnum::NoSeeds)
    }

    /// `seeds` seeds from `first` run past `u64::MAX`.
    #[must_use]
    pub fn past_last_seed(first: u64, seeds: u64) -> Self {
        Self(FuzzArgsErrorEnum::PastLastSeed(first, seeds))
    }

    /// The flag `value` is not `--versionless-reads`.
    #[must_use]
    pub fn unknown_flag(value: String) -> Self {
        Self(FuzzArgsErrorEnum::UnknownFlag(value))
    }
}
