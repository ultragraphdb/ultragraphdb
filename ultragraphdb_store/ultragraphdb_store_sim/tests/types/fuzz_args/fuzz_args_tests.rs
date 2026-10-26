/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_sim::{FuzzArgs, FuzzArgsError};

const MAX: u64 = u64::MAX;

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|a| a.to_string()).collect()
}

/// The first seed, seed count, step count and last seed `list` parses to, with 77 as the fresh seed.
fn parsed(list: &[&str]) -> (u64, u64, u64, u64) {
    let a = FuzzArgs::parse(&args(list), 77).unwrap();
    (a.first(), a.seeds(), a.steps(), a.last())
}

fn refused(list: &[&str]) -> FuzzArgsError {
    FuzzArgs::parse(&args(list), 77).unwrap_err()
}

#[test]
fn test_each_argument_count_takes_its_positions_and_defaults_the_rest() {
    assert_eq!(parsed(&[]), (77, 1000, 2000, 1076));
    assert_eq!(parsed(&["5"]), (5, 1000, 2000, 1004));
    assert_eq!(parsed(&["5", "3"]), (5, 3, 2000, 7));
    assert_eq!(parsed(&["5", "3", "10"]), (5, 3, 10, 7));
    assert_eq!(
        refused(&["5", "3", "10", "9"]),
        FuzzArgsError::TooMany(4, "9".to_string())
    );
}

#[test]
fn test_the_first_extra_argument_is_the_one_named() {
    assert_eq!(
        refused(&["5", "3", "10", "9", "8"]),
        FuzzArgsError::TooMany(4, "9".to_string())
    );
    assert_eq!(
        refused(&["5", "3", "10", "x"]),
        FuzzArgsError::TooMany(4, "x".to_string())
    );
}

#[test]
fn test_a_replay_runs_exactly_one_seed() {
    assert_eq!(parsed(&["0", "1"]), (0, 1, 2000, 0));
    assert_eq!(
        parsed(&["123456789", "1", "2000"]),
        (123456789, 1, 2000, 123456789)
    );
    assert_eq!(parsed(&["18446744073709551615", "1"]), (MAX, 1, 2000, MAX));
}

#[test]
fn test_a_non_numeric_argument_is_refused_at_its_position() {
    // The case that used to shift: `x 2 100` ran first seed 2 over 100 seeds.
    assert_eq!(
        refused(&["x", "2", "100"]),
        FuzzArgsError::NotANumber(1, "x".to_string())
    );
    assert_eq!(
        refused(&["5", "x", "100"]),
        FuzzArgsError::NotANumber(2, "x".to_string())
    );
    assert_eq!(
        refused(&["5", "2", "x"]),
        FuzzArgsError::NotANumber(3, "x".to_string())
    );
    // The leftmost bad argument is the one named.
    assert_eq!(
        refused(&["x", "y", "z", "w"]),
        FuzzArgsError::NotANumber(1, "x".to_string())
    );
}

#[test]
fn test_negative_empty_and_malformed_numbers_are_refused() {
    let cases: [(&[&str], usize, &str); 13] = [
        (&["-1"], 1, "-1"),
        (&["5", "-1"], 2, "-1"),
        (&["5", "1", "-1"], 3, "-1"),
        (&["-0"], 1, "-0"),
        (&[""], 1, ""),
        (&["5", ""], 2, ""),
        (&["5", "1", ""], 3, ""),
        (&[" 5"], 1, " 5"),
        (&["5 "], 1, "5 "),
        (&["0x10"], 1, "0x10"),
        (&["1e3"], 1, "1e3"),
        (&["1_000"], 1, "1_000"),
        (&["5.0"], 1, "5.0"),
    ];
    for (list, position, value) in cases {
        assert_eq!(
            refused(list),
            FuzzArgsError::NotANumber(position, value.to_string()),
            "{list:?}"
        );
    }
}

#[test]
fn test_a_leading_plus_is_refused_at_every_position() {
    // Go 1.27's strconv.ParseUint refuses each of these as invalid syntax, so each number has one spelling.
    let cases: [(&[&str], usize, &str); 14] = [
        (&["+5"], 1, "+5"),
        (&["5", "+5"], 2, "+5"),
        (&["5", "1", "+5"], 3, "+5"),
        // "+0" is refused before the seed count 0 is, and as steps where 0 is taken.
        (&["+0"], 1, "+0"),
        (&["5", "+0"], 2, "+0"),
        (&["5", "1", "+0"], 3, "+0"),
        (&["+"], 1, "+"),
        (&["5", "+"], 2, "+"),
        (&["5", "1", "+"], 3, "+"),
        (&["++5"], 1, "++5"),
        (&["+18446744073709551615"], 1, "+18446744073709551615"),
        (
            &["0", "1", "+18446744073709551615"],
            3,
            "+18446744073709551615",
        ),
        (&[" +5"], 1, " +5"),
        // The leftmost signed argument is the one named.
        (&["5", "+2", "+3"], 2, "+2"),
    ];
    for (list, position, value) in cases {
        assert_eq!(
            refused(list),
            FuzzArgsError::NotANumber(position, value.to_string()),
            "{list:?}"
        );
    }
}

#[test]
fn test_numbers_past_u64_are_refused_and_u64_max_is_taken() {
    let past = "18446744073709551616";
    assert_eq!(
        refused(&[past]),
        FuzzArgsError::NotANumber(1, past.to_string())
    );
    assert_eq!(
        refused(&["0", past]),
        FuzzArgsError::NotANumber(2, past.to_string())
    );
    assert_eq!(
        refused(&["0", "1", past]),
        FuzzArgsError::NotANumber(3, past.to_string())
    );
    assert_eq!(parsed(&["0", "1", "18446744073709551615"]), (0, 1, MAX, 0));
    assert_eq!(
        parsed(&["0", "18446744073709551615"]),
        (0, MAX, 2000, MAX - 1)
    );
    assert_eq!(parsed(&["1", "18446744073709551615"]), (1, MAX, 2000, MAX));
    assert_eq!(
        refused(&["2", "18446744073709551615"]),
        FuzzArgsError::PastLastSeed(2, MAX)
    );
}

#[test]
fn test_seeds_reaching_the_last_seed_are_taken_and_one_more_is_refused() {
    assert_eq!(
        parsed(&["18446744073709551606", "10"]),
        (MAX - 9, 10, 2000, MAX)
    );
    assert_eq!(
        refused(&["18446744073709551606", "11"]),
        FuzzArgsError::PastLastSeed(MAX - 9, 11)
    );
    // The default seed count applies to an explicit first seed too.
    assert_eq!(
        parsed(&["18446744073709550616"]),
        (MAX - 999, 1000, 2000, MAX)
    );
    assert_eq!(
        refused(&["18446744073709550617"]),
        FuzzArgsError::PastLastSeed(MAX - 998, 1000)
    );
    assert_eq!(
        refused(&["18446744073709551615"]),
        FuzzArgsError::PastLastSeed(MAX, 1000)
    );
}

#[test]
fn test_a_fresh_seed_is_held_to_the_same_bound() {
    let a = FuzzArgs::parse(&[], MAX - 999).unwrap();
    assert_eq!((a.first(), a.last()), (MAX - 999, MAX));
    assert_eq!(
        FuzzArgs::parse(&[], MAX).unwrap_err(),
        FuzzArgsError::PastLastSeed(MAX, 1000)
    );
    assert_eq!(FuzzArgs::parse(&[], 0).unwrap().first(), 0);
}

#[test]
fn test_zero_seeds_are_refused_and_zero_steps_taken() {
    assert_eq!(refused(&["5", "0"]), FuzzArgsError::NoSeeds());
    assert_eq!(refused(&["5", "0", "10"]), FuzzArgsError::NoSeeds());
    assert_eq!(parsed(&["5", "1", "0"]), (5, 1, 0, 5));
}

#[test]
fn test_the_seeds_run_are_first_to_last_inclusive() {
    let a = FuzzArgs::parse(&args(&["18446744073709551613", "3"]), 0).unwrap();
    assert_eq!(
        a.seeds_run().collect::<Vec<_>>(),
        vec![MAX - 2, MAX - 1, MAX]
    );
    let b = FuzzArgs::parse(&args(&["9", "1"]), 0).unwrap();
    assert_eq!(b.seeds_run().collect::<Vec<_>>(), vec![9]);
}

#[test]
fn test_the_refusal_texts_name_the_argument_and_its_position() {
    assert_eq!(
        refused(&["x", "2", "100"]).to_string(),
        "argument 1 (first seed) takes a whole number from 0 to 18446744073709551615, not \"x\"; \
         usage: store_sim_fuzz [--versionless-reads] [first seed] [seeds] [steps]"
    );
    assert_eq!(
        refused(&["5", "-1"]).to_string(),
        "argument 2 (seeds) takes a whole number from 1 to 18446744073709551615, not \"-1\"; \
         usage: store_sim_fuzz [--versionless-reads] [first seed] [seeds] [steps]"
    );
    assert_eq!(
        refused(&["5", "+5"]).to_string(),
        "argument 2 (seeds) takes a whole number from 1 to 18446744073709551615, not \"+5\"; \
         usage: store_sim_fuzz [--versionless-reads] [first seed] [seeds] [steps]"
    );
    assert_eq!(
        refused(&["5", "1", ""]).to_string(),
        "argument 3 (steps) takes a whole number from 0 to 18446744073709551615, not \"\"; \
         usage: store_sim_fuzz [--versionless-reads] [first seed] [seeds] [steps]"
    );
    assert_eq!(
        refused(&["5", "3", "10", "9"]).to_string(),
        "argument 4 (\"9\") is one past the three positions; \
         usage: store_sim_fuzz [--versionless-reads] [first seed] [seeds] [steps]"
    );
    assert_eq!(
        refused(&["5", "0"]).to_string(),
        "argument 2 (seeds) is 0, and a run takes at least 1 seed; \
         usage: store_sim_fuzz [--versionless-reads] [first seed] [seeds] [steps]"
    );
    assert_eq!(
        refused(&["18446744073709551606", "11"]).to_string(),
        "11 seeds from first seed 18446744073709551606 pass the last seed 18446744073709551615, \
         and at most 10 start there; usage: store_sim_fuzz [--versionless-reads] [first seed] [seeds] [steps]"
    );
}

const FLAG: &str = "--versionless-reads";

/// The flag comes first and leaves the numbers their positions; without it, the readers read no key without a
/// version, as every earlier run's did.
#[test]
fn test_the_versionless_reads_flag_comes_first_and_keeps_the_positions() {
    let reads = |list: &[&str]| {
        FuzzArgs::parse(&args(list), 77)
            .unwrap()
            .versionless_reads()
    };
    assert!(!reads(&[]));
    assert!(!reads(&["5", "3", "10"]));
    assert!(reads(&[FLAG]));
    assert!(reads(&[FLAG, "5", "3", "10"]));
    assert_eq!(parsed(&[FLAG]), (77, 1000, 2000, 1076));
    assert_eq!(parsed(&[FLAG, "5", "3", "10"]), (5, 3, 10, 7));
    assert_eq!(
        refused(&[FLAG, "5", "3", "10", "9"]),
        FuzzArgsError::TooMany(4, "9".to_string())
    );
    assert_eq!(
        refused(&[FLAG, "x"]),
        FuzzArgsError::NotANumber(1, "x".to_string())
    );
    // Past the first argument the flag is no number at its position, and given twice its second is one.
    assert_eq!(
        refused(&["5", FLAG]),
        FuzzArgsError::NotANumber(2, FLAG.to_string())
    );
    assert_eq!(
        refused(&[FLAG, FLAG]),
        FuzzArgsError::NotANumber(1, FLAG.to_string())
    );
}

/// A first argument starting with `--` other than the flag is refused before any number is read; a single `-` makes
/// a number, refused as one.
#[test]
fn test_an_unknown_flag_is_refused() {
    for flag in [
        "--",
        "--versionless",
        "--versionless-reads=1",
        "--VERSIONLESS-READS",
    ] {
        assert_eq!(
            refused(&[flag, "x"]),
            FuzzArgsError::UnknownFlag(flag.to_string())
        );
    }
    assert_eq!(
        refused(&["-5"]),
        FuzzArgsError::NotANumber(1, "-5".to_string())
    );
}
