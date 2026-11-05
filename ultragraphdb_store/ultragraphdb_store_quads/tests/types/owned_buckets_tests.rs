/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use ultragraphdb_core_ids::{Bucket, Tenant};
use ultragraphdb_core_mapping::RandomSource;
use ultragraphdb_store_quads::OwnedBuckets;
use ultragraphdb_tool_bdd::{Context, TestResult};

/// The first bucket of tenant 0's run: the top byte of the xxHash reference's XXH3-64 of eight zero bytes, 0xC7
/// (core_ids' `testdata/golden/buckets.json`, "the scenario: tenant 0").
const RUN_0: u32 = 0xC7_0000;
/// The first bucket of tenant 1's run: 0xF3, from the same file.
const RUN_1: u32 = 0xF3_0000;

fn bucket(b: u32) -> TestResult<Bucket> {
    Ok(Bucket::new(b)?)
}

fn owned(ranges: &[(u32, u32)]) -> TestResult<OwnedBuckets> {
    let ranges: Vec<(Bucket, Bucket)> = ranges
        .iter()
        .map(|&(a, b)| Ok((bucket(a)?, bucket(b)?)))
        .collect::<TestResult<_>>()?;
    OwnedBuckets::new(&ranges).context("valid ranges")
}

#[test]
fn test_all_owns_every_bucket() -> TestResult {
    let all = OwnedBuckets::all();
    for b in [0, 1, RUN_0, Bucket::MAX.get()] {
        assert!(all.owns(bucket(b)?), "{b:#x}");
    }
    Ok(())
}

#[test]
fn test_ranges_own_their_bounds_and_nothing_beside() -> TestResult {
    let o = owned(&[(10, 19), (20, 20), (40, 49)])?;
    for b in [10, 19, 20, 40, 49] {
        assert!(o.owns(bucket(b)?), "{b}");
    }
    for b in [0, 9, 21, 39, 50] {
        assert!(!o.owns(bucket(b)?), "{b}");
    }
    Ok(())
}

#[test]
fn test_reversed_overlapping_or_unsorted_ranges_are_refused() -> TestResult {
    let pair = |a, b| -> TestResult<(Bucket, Bucket)> { Ok((bucket(a)?, bucket(b)?)) };
    assert_eq!(OwnedBuckets::new(&[pair(5, 4)?]), None, "reversed");
    assert_eq!(
        OwnedBuckets::new(&[pair(0, 10)?, pair(10, 20)?]),
        None,
        "overlapping"
    );
    assert_eq!(
        OwnedBuckets::new(&[pair(30, 40)?, pair(0, 10)?]),
        None,
        "unsorted"
    );
    assert!(
        OwnedBuckets::new(&[pair(0, 9)?, pair(10, 20)?]).is_some(),
        "adjacent"
    );
    assert!(OwnedBuckets::new(&[]).is_some(), "no bucket");
    Ok(())
}

#[test]
fn test_a_draw_falls_among_the_owned_buckets_of_the_tenants_run() -> TestResult {
    // Partition 1 of the term-dictionary scenario: the first 21,846 buckets of tenant 0's run.
    let o = owned(&[(RUN_0, RUN_0 + 21_845)])?;
    let mut random = RandomSource::new(11);
    let mut drawn = BTreeSet::new();
    for _ in 0..2_000 {
        let b = o
            .draw(Tenant::new(0), &mut random)
            .context("a bucket of the run")?
            .get();
        assert!((RUN_0..RUN_0 + 21_846).contains(&b), "{b:#x}");
        let _new: bool = drawn.insert(b);
    }
    assert!(
        drawn.len() > 1_500,
        "{} distinct of 2,000 draws",
        drawn.len()
    );
    Ok(())
}

#[test]
fn test_a_draw_takes_the_seeds_values() -> TestResult {
    // SplitMix64 seeded 0 first gives 0xe220a8397b1dcdaf (the reference splitmix64.c); Lemire's reduction of it below
    // 21,846 is its product's high 64 bits, 19,296 (computed by hand in Python), so the bucket is 0xC7_0000 + 19,296.
    let o = owned(&[(RUN_0, RUN_0 + 21_845)])?;
    let drawn = o
        .draw(Tenant::new(0), &mut RandomSource::new(0))
        .context("a bucket")?;
    assert_eq!(drawn.get(), RUN_0 + 19_296);
    Ok(())
}

#[test]
fn test_one_seed_draws_one_sequence() -> TestResult {
    let o = owned(&[(RUN_0, RUN_0 + 99), (RUN_0 + 500, RUN_0 + 599)])?;
    let draws = |seed| -> Vec<Option<Bucket>> {
        let mut random = RandomSource::new(seed);
        (0..20)
            .map(|_| o.draw(Tenant::new(0), &mut random))
            .collect()
    };
    assert_eq!(draws(3), draws(3));
    assert_ne!(draws(3), draws(4));
    Ok(())
}

#[test]
fn test_every_range_is_drawn_from() -> TestResult {
    let o = owned(&[(RUN_0, RUN_0 + 99), (RUN_0 + 500, RUN_0 + 599)])?;
    let mut random = RandomSource::new(5);
    let (mut low, mut high) = (0, 0);
    for _ in 0..1_000 {
        match o
            .draw(Tenant::new(0), &mut random)
            .context("a bucket")?
            .get()
        {
            b if b < RUN_0 + 100 => low += 1,
            b if (RUN_0 + 500..RUN_0 + 600).contains(&b) => high += 1,
            b => {
                return Err(ultragraphdb_tool_bdd::TestError::new(format!(
                    "{b:#x} is not owned"
                )));
            }
        }
    }
    // Two ranges of 100 buckets each: about half the draws each.
    assert!((350..=650).contains(&low), "{low} of 1,000");
    assert_eq!(low + high, 1_000);
    Ok(())
}

#[test]
fn test_only_the_tenants_run_is_drawn_from() -> TestResult {
    // One range straddling the end of tenant 0's run: only its part inside the run is drawn.
    let o = owned(&[(RUN_0 + 0xFFF0, RUN_0 + 0x1_000F)])?;
    let mut random = RandomSource::new(9);
    for _ in 0..200 {
        let b = o
            .draw(Tenant::new(0), &mut random)
            .context("a bucket")?
            .get();
        assert!((RUN_0 + 0xFFF0..RUN_0 + 0x1_0000).contains(&b), "{b:#x}");
    }
    Ok(())
}

#[test]
fn test_a_single_owned_bucket_is_always_drawn() -> TestResult {
    let o = owned(&[(RUN_1 + 7, RUN_1 + 7)])?;
    let mut random = RandomSource::new(1);
    for _ in 0..10 {
        assert_eq!(
            o.draw(Tenant::new(1), &mut random),
            Some(bucket(RUN_1 + 7)?)
        );
    }
    Ok(())
}

#[test]
fn test_no_owned_bucket_of_the_run_draws_nothing() -> TestResult {
    let o = owned(&[(RUN_1, RUN_1 + 0xFFFF)])?;
    assert_eq!(o.draw(Tenant::new(0), &mut RandomSource::new(1)), None);
    assert_eq!(
        OwnedBuckets::new(&[])
            .context("no bucket")?
            .draw(Tenant::new(0), &mut RandomSource::new(1)),
        None
    );
    assert!(
        OwnedBuckets::all()
            .draw(Tenant::new(1), &mut RandomSource::new(1))
            .is_some()
    );
    Ok(())
}
