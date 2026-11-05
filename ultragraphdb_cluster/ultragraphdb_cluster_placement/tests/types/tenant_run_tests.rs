/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::TenantRun;
use ultragraphdb_core_ids::{Bucket, BucketGroup, CanonicalKey, Tenant, TypePrefix};
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_a_run_holds_65536_buckets_in_1024_groups() {
    assert_eq!(TenantRun::BUCKETS, 65_536);
    assert_eq!(TenantRun::GROUPS, 1_024);
}

#[test]
fn test_a_tenants_run_is_its_derived_buckets_top_bits() -> TestResult {
    for tenant in [0, 1, 2, u64::MAX] {
        let tenant = Tenant::new(tenant);
        let bucket = Bucket::derive(
            tenant,
            TypePrefix::RESOURCE,
            &CanonicalKey::iri("http://ex.org/a")?,
        );
        assert_eq!(u32::from(TenantRun::of(tenant).get()), bucket.get() >> 16);
        assert_eq!(TenantRun::of(tenant), TenantRun::of_bucket(bucket));
    }
    Ok(())
}

#[test]
fn test_the_first_and_the_last_runs() -> TestResult {
    assert_eq!(TenantRun::of_bucket(Bucket::new(0)?).get(), 0);
    assert_eq!(TenantRun::of_bucket(Bucket::new(65_535)?).get(), 0);
    assert_eq!(TenantRun::of_bucket(Bucket::new(65_536)?).get(), 1);
    assert_eq!(TenantRun::of_bucket(Bucket::MAX).get(), 255);
    Ok(())
}

#[test]
fn test_a_runs_range() {
    let first = TenantRun::new(0).range();
    assert_eq!((first.start(), first.end()), (0, 65_536));
    // 0xAB << 16 = 11,206,656.
    let middle = TenantRun::new(0xAB).range();
    assert_eq!((middle.start(), middle.end()), (11_206_656, 11_272_192));
    let last = TenantRun::new(255).range();
    assert_eq!((last.start(), last.end()), (16_711_680, 16_777_216));
}

#[test]
fn test_a_groups_run() -> TestResult {
    // A run holds 1,024 groups: group 1,023 is the last of run 0, 1,024 the first of run 1, 262,143 in run 255.
    assert_eq!(TenantRun::of_group(BucketGroup::new(342)?).get(), 0);
    assert_eq!(TenantRun::of_group(BucketGroup::new(1_023)?).get(), 0);
    assert_eq!(TenantRun::of_group(BucketGroup::new(1_024)?).get(), 1);
    assert_eq!(TenantRun::of_group(BucketGroup::new(262_143)?).get(), 255);
    Ok(())
}
