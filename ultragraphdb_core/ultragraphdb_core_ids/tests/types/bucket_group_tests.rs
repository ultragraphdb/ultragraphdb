/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Bucket, BucketGroup, IdErrorEnum};
use ultragraphdb_tool_bdd::{Context, TestResult};

#[test]
fn test_a_group_holds_64_buckets_of_2_to_the_18() {
    assert_eq!(BucketGroup::BUCKETS, 64);
    assert_eq!(BucketGroup::COUNT, 262_144);
}

#[test]
fn test_groups_zero_and_the_largest_hold_their_value() -> TestResult {
    assert_eq!(BucketGroup::new(0)?.get(), 0);
    assert_eq!(BucketGroup::new(262_143)?.get(), 262_143);
    Ok(())
}

#[test]
fn test_a_group_past_18_bits_is_refused() -> TestResult {
    for group in [262_144, u32::MAX] {
        let refusal = BucketGroup::new(group).err().context("a refusal")?;
        assert_eq!(refusal.0, IdErrorEnum::BucketGroupRange { group });
    }
    Ok(())
}

#[test]
fn test_a_buckets_group_is_its_top_18_bits() -> TestResult {
    assert_eq!(BucketGroup::of(Bucket::new(0)?).get(), 0);
    assert_eq!(BucketGroup::of(Bucket::new(63)?).get(), 0);
    assert_eq!(BucketGroup::of(Bucket::new(64)?).get(), 1);
    // 21,888 = 342 * 64 and 21,951 its group's last bucket.
    assert_eq!(BucketGroup::of(Bucket::new(21_888)?).get(), 342);
    assert_eq!(BucketGroup::of(Bucket::new(21_951)?).get(), 342);
    assert_eq!(BucketGroup::of(Bucket::MAX).get(), 262_143);
    Ok(())
}

#[test]
fn test_a_groups_first_bucket_is_64_times_the_group() -> TestResult {
    // 342 * 64 = 21,888; 262,143 * 64 = 16,777,152, the last group's first of its 64 buckets up to 2^24 - 1.
    assert_eq!(BucketGroup::new(0)?.first_bucket(), Bucket::new(0)?);
    assert_eq!(BucketGroup::new(342)?.first_bucket(), Bucket::new(21_888)?);
    assert_eq!(
        BucketGroup::new(262_143)?.first_bucket(),
        Bucket::new(16_777_152)?
    );
    Ok(())
}

#[test]
fn test_a_groups_first_bucket_lies_in_the_group() -> TestResult {
    for group in [0, 1, 342, 1_023, 1_024, 262_143] {
        let group = BucketGroup::new(group)?;
        assert_eq!(BucketGroup::of(group.first_bucket()), group);
    }
    Ok(())
}
