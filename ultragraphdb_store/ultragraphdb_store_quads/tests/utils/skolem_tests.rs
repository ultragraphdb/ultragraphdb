/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Bucket, CanonicalKey, Tenant, TypePrefix};
use ultragraphdb_store_quads::{iri_bucket, tenant_base};
use ultragraphdb_tool_bdd::{Context, TestResult};

const TENANT: u64 = 3;

/// The bucket a plain IRI's key derives.
fn derived(iri: &str) -> TestResult<Bucket> {
    Ok(Bucket::derive(
        Tenant::new(TENANT),
        TypePrefix::RESOURCE,
        &CanonicalKey::iri(iri)?,
    ))
}

#[test]
fn test_an_iris_bucket_is_its_keys() -> TestResult {
    let iri = "http://ex.org/a";
    assert_eq!(iri_bucket(TENANT, iri), Some(derived(iri)?));
    Ok(())
}

/// A skolem IRI of the tenant states its bucket, as six hexadecimal digits, whatever its key would derive; 0xABCDEF is
/// the number the name spells.
#[test]
fn test_a_skolem_iri_states_its_bucket() -> TestResult {
    let skolem = format!(
        "{}.well-known/genid/abcdef-{}",
        tenant_base(TENANT),
        "1".repeat(26)
    );
    let bucket = iri_bucket(TENANT, &skolem).context("a bucket")?;
    assert_eq!(bucket.get(), 0xAB_CDEF);
    assert_ne!(bucket, derived(&skolem)?);
    Ok(())
}

/// A name that only looks like a skolem IRI, another tenant's or a malformed one, is an ordinary IRI: its key derives
/// its bucket.
#[test]
fn test_what_only_looks_like_a_skolem_iri_derives_from_its_key() -> TestResult {
    let tail = format!(".well-known/genid/abcdef-{}", "1".repeat(26));
    for iri in [
        format!("{}{tail}", tenant_base(TENANT + 1)),
        format!("{}.well-known/genid/abcdef-short", tenant_base(TENANT)),
        format!("http://ex.org/{tail}"),
    ] {
        assert_eq!(iri_bucket(TENANT, &iri), Some(derived(&iri)?), "{iri}");
    }
    Ok(())
}
