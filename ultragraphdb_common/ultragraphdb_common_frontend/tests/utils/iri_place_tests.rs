/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_frontend::{IriPlace, iri_place};
use ultragraphdb_core_ids::{Bucket, CanonicalKey, Tenant, TypePrefix};
use ultragraphdb_core_terms::SYSTEM_VOCABULARY;
use ultragraphdb_tool_bdd::TestResult;

fn skolem(tenant: u64, tail: &str) -> String {
    format!("{SYSTEM_VOCABULARY}tenant/{tenant}/.well-known/genid/{tail}")
}

#[test]
fn test_an_ordinary_iri_lies_where_its_text_derives() -> TestResult {
    let iri = "http://ex.org/a";
    let derived = Bucket::derive(
        Tenant::new(3),
        TypePrefix::RESOURCE,
        &CanonicalKey::iri(iri)?,
    );
    assert_eq!(iri_place(3, iri), IriPlace::Ordinary(derived));
    Ok(())
}

#[test]
fn test_a_tenants_skolem_iri_lies_where_its_text_states() -> TestResult {
    let run = u32::from(Tenant::new(0).bucket_bits()) << 16;
    let iri = skolem(0, &format!("{run:06x}-00000000000000000000000000"));
    assert_eq!(iri_place(0, &iri), IriPlace::Skolem(Bucket::new(run)?));
    Ok(())
}

#[test]
fn test_a_skolem_iri_nobody_can_hold() {
    let run = u32::from(Tenant::new(0).bucket_bits()) << 16;
    let outside = run.wrapping_add(1 << 16) & 0xFF_FFFF;
    for iri in [
        skolem(7, &format!("{run:06x}-00000000000000000000000000")),
        skolem(0, "nothing"),
        skolem(0, &format!("{outside:06x}-00000000000000000000000000")),
    ] {
        assert_eq!(iri_place(0, &iri), IriPlace::Nobody, "{iri}");
    }
}

#[test]
fn test_the_same_path_under_another_base_is_ordinary() {
    let iri = "https://example.org/tenant/0/.well-known/genid/00abcd-0123456789abcdef0123456789";
    assert!(matches!(iri_place(0, iri), IriPlace::Ordinary(_)));
}
