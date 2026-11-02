/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::BucketGroup;
use ultragraphdb_core_ids::Bucket;
use ultragraphdb_error_cluster::ClusterErrorEnum;
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
        assert_eq!(refusal.0, ClusterErrorEnum::BucketGroupRange { group });
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
fn test_a_groups_range_and_run() -> TestResult {
    let group = BucketGroup::new(342)?;
    assert_eq!(
        (group.range().start(), group.range().end()),
        (21_888, 21_952)
    );
    assert_eq!(group.run().get(), 0);
    // Group 1,023 is the last of run 0, group 1,024 the first of run 1.
    assert_eq!(BucketGroup::new(1_023)?.run().get(), 0);
    assert_eq!(BucketGroup::new(1_024)?.run().get(), 1);
    let last = BucketGroup::new(262_143)?;
    assert_eq!(
        (last.range().start(), last.range().end()),
        (16_777_152, 16_777_216)
    );
    assert_eq!(last.run().get(), 255);
    Ok(())
}
