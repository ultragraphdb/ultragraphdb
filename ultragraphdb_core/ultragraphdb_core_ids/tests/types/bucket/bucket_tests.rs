/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use serde::Deserialize;
use ultragraphdb_core_ids::{Bucket, CanonicalKey, IdError, IdErrorEnum, Namespace, TypePrefix};
use ultragraphdb_tool_bdd::{Context, Golden, TestResult, load_golden, testdata_dir};

/// A golden bucket: the xxHash reference implementation's XXH3-64, through python-xxhash, over byte strings built by
/// `testdata/golden/buckets.py` from the rule's text alone.
#[derive(Debug, Deserialize)]
struct Case {
    note: String,
    namespace: u64,
    prefix: u16,
    iri: String,
    namespace_xxh3: String,
    key_xxh3: String,
    bucket: u32,
}

fn golden() -> TestResult<Golden<Case>> {
    load_golden(&testdata_dir()?.join("golden").join("buckets.json"))
}

#[test]
fn test_buckets_zero_and_the_largest_hold_their_value() -> TestResult {
    assert_eq!(Bucket::new(0)?.get(), 0);
    // 2^24 − 1, by hand.
    assert_eq!(Bucket::new(16_777_215)?, Bucket::MAX);
    assert_eq!(Bucket::MAX.get(), 16_777_215);
    Ok(())
}

#[test]
fn test_a_bucket_past_24_bits_is_refused() {
    assert_eq!(
        Bucket::new(16_777_216),
        Err(IdError(IdErrorEnum::BucketRange { bucket: 16_777_216 }))
    );
    assert_eq!(
        Bucket::new(u32::MAX),
        Err(IdError(IdErrorEnum::BucketRange { bucket: u32::MAX }))
    );
}

#[test]
fn test_every_golden_bucket_matches_the_reference() -> TestResult {
    let golden = golden()?;
    assert!(
        golden.generator().contains("python-xxhash"),
        "{}",
        golden.generator()
    );
    assert_eq!(golden.cases().len(), 14);
    for case in golden.cases() {
        let key = CanonicalKey::iri(&case.iri)?;
        let bucket = Bucket::derive(
            Namespace::new(case.namespace),
            TypePrefix::new(case.prefix)?,
            &key,
        );
        assert_eq!(bucket.get(), case.bucket, "{}", case.note);
    }
    Ok(())
}

#[test]
fn test_the_golden_buckets_are_the_reference_hashes_top_bits() -> TestResult {
    // The bucket's two halves read straight from the reference's hashes, not from the golden bucket.
    for case in golden()?.cases() {
        let namespace_hash = u64::from_str_radix(&case.namespace_xxh3, 16).context("a hex hash")?;
        let key_hash = u64::from_str_radix(&case.key_xxh3, 16).context("a hex hash")?;
        let key = CanonicalKey::iri(&case.iri)?;
        let bucket = Bucket::derive(
            Namespace::new(case.namespace),
            TypePrefix::new(case.prefix)?,
            &key,
        )
        .get();
        assert_eq!(
            u64::from(bucket >> 16),
            namespace_hash >> 56,
            "{}",
            case.note
        );
        assert_eq!(u64::from(bucket & 0xFFFF), key_hash >> 48, "{}", case.note);
    }
    Ok(())
}

#[test]
fn test_the_scenario_bucket_of_http_ex_org_a() -> TestResult {
    // buckets.json, "the scenario: namespace 0": XXH3-64 of eight zero bytes is 0xc77b3abb6f87acd9, of 00 01, 00 00 00
    // 0F and the IRI 0xf1ab1116c6f475ad.
    let key = CanonicalKey::iri("http://ex.org/a")?;
    let bucket = Bucket::derive(Namespace::new(0), TypePrefix::RESOURCE, &key);
    assert_eq!(bucket.get(), 0xC7_F1AB);
    Ok(())
}

#[test]
fn test_a_bucket_is_the_same_every_time() -> TestResult {
    let key = CanonicalKey::iri("http://ex.org/a")?;
    let first = Bucket::derive(Namespace::new(5), TypePrefix::RESOURCE, &key);
    let again = Bucket::derive(
        Namespace::new(5),
        TypePrefix::RESOURCE,
        &CanonicalKey::iri("http://ex.org/a")?,
    );
    assert_eq!(first, again);
    Ok(())
}

#[test]
fn test_buckets_order_as_numbers() -> TestResult {
    assert!(Bucket::new(9)? < Bucket::new(900)?);
    assert!(Bucket::new(0)? < Bucket::MAX);
    Ok(())
}

/// The predicate the value tests name: type 1, sequence 42, bucket 5, `(1 << 112) | (42 << 48) | (5 << 24)`.
fn value_predicate() -> Result<ultragraphdb_core_ids::SystemId, ultragraphdb_core_ids::IdError> {
    ultragraphdb_core_ids::SystemId::from_u128((1 << 112) | (42 << 48) | (5 << 24))
}

#[test]
fn test_a_unique_values_bucket_is_its_namespaces_top_and_its_predicate_and_values_hash()
-> Result<(), ultragraphdb_core_ids::IdError> {
    use ultragraphdb_core_ids::{Bucket, Namespace};
    // python-xxhash 3.6.0: XXH3-64 of namespace 0's 8 big-endian bytes has top byte 0xC7; of the predicate's 16 bytes
    // followed by b"x", top two bytes 0xB4D0; followed by nothing, 0x75ED.
    let predicate = value_predicate()?;
    assert_eq!(
        Bucket::of_value(Namespace::new(0), predicate, b"x").get(),
        0xC7_B4D0
    );
    assert_eq!(
        Bucket::of_value(Namespace::new(0), predicate, b"").get(),
        0xC7_75ED
    );
    // Namespace 7's top byte, 0xF8, with the same low 16 bits.
    assert_eq!(
        Bucket::of_value(Namespace::new(7), predicate, b"x").get(),
        0xF8_B4D0
    );
    Ok(())
}
