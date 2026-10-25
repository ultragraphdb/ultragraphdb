/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod fuzz_args_error_display;
mod fuzz_args_error_error;

/// Why `store_sim_fuzz` refused its arguments. A position counts from 1, the program's name left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuzzArgsError(pub FuzzArgsErrorEnum);

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
}

#[allow(non_snake_case)]
impl FuzzArgsError {
    pub fn NotANumber(position: usize, value: String) -> Self {
        Self(FuzzArgsErrorEnum::NotANumber(position, value))
    }

    pub fn TooMany(position: usize, value: String) -> Self {
        Self(FuzzArgsErrorEnum::TooMany(position, value))
    }

    pub fn NoSeeds() -> Self {
        Self(FuzzArgsErrorEnum::NoSeeds)
    }

    pub fn PastLastSeed(first: u64, seeds: u64) -> Self {
        Self(FuzzArgsErrorEnum::PastLastSeed(first, seeds))
    }
}
