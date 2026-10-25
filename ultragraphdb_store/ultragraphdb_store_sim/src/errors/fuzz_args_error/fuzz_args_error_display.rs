/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::fuzz_args_error::{FuzzArgsError, FuzzArgsErrorEnum};

const USAGE: &str = "usage: store_sim_fuzz [first seed] [seeds] [steps]";

/// The name of the argument at `position`, counted from 1.
fn name(position: usize) -> &'static str {
    match position {
        1 => "first seed",
        2 => "seeds",
        3 => "steps",
        _ => "no position",
    }
}

/// The smallest number the argument at `position` takes: a run takes at least one seed.
fn least(position: usize) -> u64 {
    if position == 2 { 1 } else { 0 }
}

impl fmt::Display for FuzzArgsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            FuzzArgsErrorEnum::NotANumber(position, value) => write!(
                f,
                "argument {position} ({}) takes a whole number from {} to {}, not {value:?}; {USAGE}",
                name(*position),
                least(*position),
                u64::MAX
            ),
            FuzzArgsErrorEnum::TooMany(position, value) => write!(
                f,
                "argument {position} ({value:?}) is one past the three positions; {USAGE}"
            ),
            FuzzArgsErrorEnum::NoSeeds => write!(
                f,
                "argument 2 (seeds) is 0, and a run takes at least 1 seed; {USAGE}"
            ),
            FuzzArgsErrorEnum::PastLastSeed(first, seeds) => write!(
                f,
                "{seeds} seeds from first seed {first} pass the last seed {}, and at most {} start there; {USAGE}",
                u64::MAX,
                (u64::MAX - first).saturating_add(1)
            ),
        }
    }
}
