/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_store_sim::{FuzzArgsError, FuzzArgsErrorEnum};

const USAGE: &str = "usage: store_sim_fuzz [first seed] [seeds] [steps]";

#[test]
fn test_each_constructor_builds_its_variant() {
    assert_eq!(
        FuzzArgsError::NotANumber(2, "x".to_string()).0,
        FuzzArgsErrorEnum::NotANumber(2, "x".to_string())
    );
    assert_eq!(
        FuzzArgsError::TooMany(4, "9".to_string()).0,
        FuzzArgsErrorEnum::TooMany(4, "9".to_string())
    );
    assert_eq!(FuzzArgsError::NoSeeds().0, FuzzArgsErrorEnum::NoSeeds);
    assert_eq!(
        FuzzArgsError::PastLastSeed(7, 8).0,
        FuzzArgsErrorEnum::PastLastSeed(7, 8)
    );
}

#[test]
fn test_each_position_is_named() {
    let text = |position| FuzzArgsError::NotANumber(position, "x".to_string()).to_string();
    assert_eq!(
        text(1),
        format!(
            "argument 1 (first seed) takes a whole number from 0 to 18446744073709551615, not \"x\"; {USAGE}"
        )
    );
    assert_eq!(
        text(2),
        format!(
            "argument 2 (seeds) takes a whole number from 1 to 18446744073709551615, not \"x\"; {USAGE}"
        )
    );
    assert_eq!(
        text(3),
        format!(
            "argument 3 (steps) takes a whole number from 0 to 18446744073709551615, not \"x\"; {USAGE}"
        )
    );
}

#[test]
fn test_a_position_outside_the_three_has_no_name() {
    assert_eq!(
        FuzzArgsError::NotANumber(0, "x".to_string()).to_string(),
        format!(
            "argument 0 (no position) takes a whole number from 0 to 18446744073709551615, not \"x\"; {USAGE}"
        )
    );
    assert_eq!(
        FuzzArgsError::NotANumber(4, "x".to_string()).to_string(),
        format!(
            "argument 4 (no position) takes a whole number from 0 to 18446744073709551615, not \"x\"; {USAGE}"
        )
    );
}

#[test]
fn test_extra_arguments_are_named_by_position_and_value() {
    assert_eq!(
        FuzzArgsError::TooMany(4, "9".to_string()).to_string(),
        format!("argument 4 (\"9\") is one past the three positions; {USAGE}")
    );
    // A value is quoted with its escapes, so an empty or control-character argument stays visible.
    assert_eq!(
        FuzzArgsError::TooMany(4, String::new()).to_string(),
        format!("argument 4 (\"\") is one past the three positions; {USAGE}")
    );
    assert_eq!(
        FuzzArgsError::NotANumber(1, "a\tb\u{fffd}".to_string()).to_string(),
        format!(
            "argument 1 (first seed) takes a whole number from 0 to 18446744073709551615, not \"a\\tb\u{fffd}\"; {USAGE}"
        )
    );
}

#[test]
fn test_zero_seeds_and_a_range_past_the_last_seed_say_why() {
    assert_eq!(
        FuzzArgsError::NoSeeds().to_string(),
        format!("argument 2 (seeds) is 0, and a run takes at least 1 seed; {USAGE}")
    );
    assert_eq!(
        FuzzArgsError::PastLastSeed(u64::MAX, 1000).to_string(),
        format!(
            "1000 seeds from first seed 18446744073709551615 pass the last seed 18446744073709551615, \
             and at most 1 start there; {USAGE}"
        )
    );
    assert_eq!(
        FuzzArgsError::PastLastSeed(2, u64::MAX).to_string(),
        format!(
            "18446744073709551615 seeds from first seed 2 pass the last seed 18446744073709551615, \
             and at most 18446744073709551614 start there; {USAGE}"
        )
    );
}

#[test]
fn test_a_refusal_is_an_error_with_no_source() {
    assert!(FuzzArgsError::NoSeeds().source().is_none());
    assert!(
        FuzzArgsError::TooMany(4, "9".to_string())
            .source()
            .is_none()
    );
}
