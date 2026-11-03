/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::BucketRange;
use ultragraphdb_core_ids::{Bucket, BucketGroup};
use ultragraphdb_error_cluster::ClusterErrorEnum;
use ultragraphdb_tool_bdd::{Context, TestResult};

#[test]
fn test_the_end_is_two_to_the_24() {
    // The bucket after the last of 24 bits.
    assert_eq!(BucketRange::END, 16_777_216);
}

#[test]
fn test_a_range_keeps_its_bounds_and_length() -> TestResult {
    let range = BucketRange::new(21_846, 43_691)?;
    assert_eq!(range.start(), 21_846);
    assert_eq!(range.end(), 43_691);
    // 43,691 - 21,846.
    assert_eq!(range.buckets(), 21_845);
    Ok(())
}

#[test]
fn test_the_smallest_and_the_whole_range() -> TestResult {
    assert_eq!(BucketRange::new(0, 1)?.buckets(), 1);
    assert_eq!(BucketRange::new(16_777_215, 16_777_216)?.buckets(), 1);
    assert_eq!(BucketRange::new(0, 16_777_216)?.buckets(), 16_777_216);
    Ok(())
}

#[test]
fn test_an_empty_or_reversed_range_is_refused() -> TestResult {
    for (start, end) in [(5, 5), (0, 0), (7, 3), (16_777_216, 16_777_216)] {
        let refusal = BucketRange::new(start, end).err().context("a refusal")?;
        assert_eq!(refusal.0, ClusterErrorEnum::BucketRange { start, end });
    }
    Ok(())
}

#[test]
fn test_a_range_past_the_last_bucket_is_refused() -> TestResult {
    for (start, end) in [(0, 16_777_217), (16_777_215, u32::MAX)] {
        let refusal = BucketRange::new(start, end).err().context("a refusal")?;
        assert_eq!(refusal.0, ClusterErrorEnum::BucketRange { start, end });
    }
    Ok(())
}

#[test]
fn test_a_range_contains_its_buckets_alone() -> TestResult {
    let range = BucketRange::new(100, 200)?;
    assert!(!range.contains(Bucket::new(99)?));
    assert!(range.contains(Bucket::new(100)?));
    assert!(range.contains(Bucket::new(199)?));
    assert!(!range.contains(Bucket::new(200)?));
    Ok(())
}

#[test]
fn test_a_groups_range_is_its_64_buckets() -> TestResult {
    // 342 * 64 = 21,888 and 262,143 * 64 = 16,777,152, each followed by the group's 64 buckets.
    let range = BucketRange::of_group(BucketGroup::new(342)?);
    assert_eq!((range.start(), range.end()), (21_888, 21_952));
    let first = BucketRange::of_group(BucketGroup::new(0)?);
    assert_eq!((first.start(), first.end()), (0, 64));
    let last = BucketRange::of_group(BucketGroup::new(262_143)?);
    assert_eq!((last.start(), last.end()), (16_777_152, 16_777_216));
    Ok(())
}
