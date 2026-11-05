/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use serde::Deserialize;
use ultragraphdb_core_ids::{Bucket, CanonicalKey, Tenant, TypePrefix};
use ultragraphdb_tool_bdd::{Context, TestResult, load_golden, testdata_dir};

/// The fields of a golden case of `testdata/golden/buckets.json` this test reads.
#[derive(Debug, Deserialize)]
struct Case {
    tenant: u64,
    tenant_xxh3: String,
}

#[test]
fn test_a_tenant_holds_its_value() {
    assert_eq!(Tenant::new(0).get(), 0);
    assert_eq!(Tenant::new(133).get(), 133);
    assert_eq!(Tenant::new(u64::MAX).get(), u64::MAX);
}

#[test]
fn test_distinct_tenants_differ() {
    assert_ne!(Tenant::new(0), Tenant::new(1));
    assert!(Tenant::new(1) < Tenant::new(2));
}

#[test]
fn test_bucket_bits_are_the_reference_hashes_top_byte() -> TestResult {
    // The golden cases' tenant hashes come from the xxHash reference (buckets.py): tenants 0, 1, 2,
    // 0x0102030405060708 and 2^64 - 1.
    let golden = load_golden::<Case>(&testdata_dir()?.join("golden").join("buckets.json"))?;
    assert!(!golden.cases().is_empty());
    for case in golden.cases() {
        let hash = u64::from_str_radix(&case.tenant_xxh3, 16).context("a hex hash")?;
        let [top, ..] = hash.to_be_bytes();
        assert_eq!(Tenant::new(case.tenant).bucket_bits(), top);
    }
    Ok(())
}

#[test]
fn test_bucket_bits_are_every_derived_buckets_top_bits() -> TestResult {
    // Derived buckets of two keys and two prefixes in one tenant share their top 8 bits with it.
    let tenant = Tenant::new(0x0102_0304_0506_0708);
    for (prefix, iri) in [
        (TypePrefix::RESOURCE, "http://ex.org/a"),
        (TypePrefix::STATEMENT, ""),
    ] {
        let bucket = Bucket::derive(tenant, prefix, &CanonicalKey::iri(iri)?);
        assert_eq!(u32::from(tenant.bucket_bits()), bucket.get() >> 16);
    }
    Ok(())
}
