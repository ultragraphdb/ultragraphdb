/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::NamedNode;
use ultragraphdb_core_ids::Bucket;
use ultragraphdb_core_mapping::{MappingError, Names, RandomSource};
use ultragraphdb_tool_bdd::{Context, TestResult};

#[test]
fn test_names_map_to_iris_under_the_base_and_back() -> TestResult {
    let names = Names::new("http://ex.org/")?;
    assert_eq!(
        names.iri("age"),
        NamedNode::new_unchecked("http://ex.org/age")
    );
    assert_eq!(names.name(&names.iri("age")), "age");
    let other = NamedNode::new_unchecked("http://other.org/age");
    assert_eq!(
        names.name(&other),
        "http://other.org/age",
        "an IRI elsewhere is its own name"
    );
    assert_eq!(names.iri("http://other.org/age"), other);
    assert_eq!(
        names.name(&NamedNode::new_unchecked("http://ex.org/")),
        "http://ex.org/"
    );
    assert_eq!(names.base(), "http://ex.org/");
    Ok(())
}

#[test]
fn test_an_iri_under_the_base_holding_a_colon_is_named_by_itself() -> TestResult {
    let names = Names::new("http://ex.org/")?;
    // Its name would read back as an absolute IRI, "a:b", not as the IRI it came from.
    let colon = NamedNode::new_unchecked("http://ex.org/a:b");
    assert_eq!(names.name(&colon), "http://ex.org/a:b");
    assert_eq!(names.iri(&names.name(&colon)), colon);
    // A path under the base is a name, and maps back.
    let nested = NamedNode::new_unchecked("http://ex.org/x/y");
    assert_eq!(names.name(&nested), "x/y");
    assert_eq!(names.iri("x/y"), nested);
    Ok(())
}

/// The refusal of `base` for `reason`.
fn refused(base: &str, reason: &'static str) -> Result<Names, MappingError> {
    Err(MappingError::invalid_base(base.to_string(), reason))
}

#[test]
fn test_a_base_that_is_no_absolute_iri_is_refused() {
    // RFC 3987's absolute-IRI begins with a scheme and ":"; none of these does, or the text is no IRI at all.
    for base in [
        "",
        "ex.org/",
        "/ns/",
        "//ex.org/",
        "http//ex.org/",
        "1http://ex.org/",
        // A space is no IRI character (RFC 3987 section 2.2).
        "http://ex.org/a b/",
    ] {
        assert_eq!(
            Names::new(base),
            refused(base, "not an absolute IRI"),
            "{base:?}"
        );
    }
}

#[test]
fn test_a_base_with_a_query_or_a_fragment_is_refused() {
    // A name appended to such a base lands in its query or fragment, and the skolem IRI's `/.well-known/genid/` path
    // (PROTOCOL.md sections 4.4 and 10.2) with it.
    for base in [
        "http://ex.org/ns#",
        "http://ex.org/a#b/",
        "http://ex.org/?q=/",
    ] {
        assert_eq!(
            Names::new(base),
            refused(base, "has a query or a fragment"),
            "{base:?}"
        );
    }
}

#[test]
fn test_a_base_not_ending_in_a_slash_is_refused() {
    // A name appended to it would run into its last segment: "http://ex.org/ns" and "age" give ".../nsage".
    for base in ["http://ex.org", "http://ex.org/ns", "urn:ex:ns", "http:"] {
        assert_eq!(
            Names::new(base),
            refused(base, "does not end in '/'"),
            "{base:?}"
        );
    }
}

#[test]
fn test_an_absolute_base_ending_in_a_slash_names_iris_under_it() -> TestResult {
    for base in [
        "http://ex.org/",
        "urn:ex:ns/",
        "file:///",
        // The design's default base of namespace 0, under the system vocabulary.
        "https://w3id.org/ultragraphdb/ns/namespace/0/",
    ] {
        let names = Names::new(base)?;
        assert_eq!(names.base(), base);
        assert_eq!(names.iri("age").as_str(), format!("{base}age"));
        assert_eq!(names.name(&names.iri("age")), "age");
    }
    // PROTOCOL.md section 10.2's skolem IRI of a blank node of namespace 0 in bucket 6, its random bits 9.
    assert_eq!(
        Names::new("https://w3id.org/ultragraphdb/ns/namespace/0/")?
            .skolem_iri(Bucket::new(6)?, 9)
            .as_str(),
        "https://w3id.org/ultragraphdb/ns/namespace/0/.well-known/genid/000006-00000000000000000000000009"
    );
    Ok(())
}

#[test]
fn test_a_unicode_base_and_unicode_names_are_iris() -> TestResult {
    // RFC 3987 takes non-ASCII characters (ucschar) unescaped, in the host and the path alike.
    let names = Names::new("http://例え.jp/名前/")?;
    assert_eq!(names.iri("年齢").as_str(), "http://例え.jp/名前/年齢");
    assert_eq!(names.name(&names.iri("年齢")), "年齢");
    assert_eq!(
        names.skolem_iri(Bucket::new(6)?, 9).as_str(),
        "http://例え.jp/名前/.well-known/genid/000006-00000000000000000000000009"
    );
    // The same rules hold for a unicode base: one without its trailing slash is refused.
    assert_eq!(
        Names::new("http://例え.jp/名前"),
        refused("http://例え.jp/名前", "does not end in '/'")
    );
    Ok(())
}

#[test]
fn test_a_skolem_iri_is_its_bucket_and_104_bits_in_lowercase_hexadecimal() -> TestResult {
    let names = Names::new("http://ex.org/")?;
    // Hand-written: 6 digits of the bucket, `-`, 26 of the random bits, both zero-padded; bits above 104 are dropped.
    for (bucket, random, want) in [
        (0, 0, "000000-00000000000000000000000000"),
        (
            0xFF_FFFF,
            (1u128 << 104) - 1,
            "ffffff-ffffffffffffffffffffffffff",
        ),
        (
            0xAB_CDEF,
            0x0001_2345_6789_ABCD_EF01_2345_6789,
            "abcdef-0123456789abcdef0123456789",
        ),
        (5, u128::MAX, "000005-ffffffffffffffffffffffffff"),
    ] {
        assert_eq!(
            names.skolem_iri(Bucket::new(bucket)?, random).as_str(),
            format!("http://ex.org/.well-known/genid/{want}")
        );
    }
    Ok(())
}

#[test]
fn test_a_skolem_iri_states_its_bucket() -> TestResult {
    let names = Names::new("http://ex.org/")?;
    for bucket in [0, 1, 0xAB_CDEF, 0xFF_FFFF] {
        let iri = names.skolem_iri(Bucket::new(bucket)?, 0x7777);
        assert_eq!(
            names.skolem_bucket(iri.as_str()),
            Some(Bucket::new(bucket)?)
        );
    }
    Ok(())
}

#[test]
fn test_only_a_well_formed_skolem_iri_under_the_base_states_a_bucket() -> TestResult {
    let names = Names::new("http://ex.org/")?;
    let random = "0123456789abcdef0123456789";
    for iri in [
        // Another base, the stage-5 form, uppercase, a short or long part, no separator, a trailing character.
        format!("http://other.org/.well-known/genid/abcdef-{random}"),
        "http://ex.org/.well-known/genid/9".to_string(),
        format!("http://ex.org/.well-known/genid/ABCDEF-{random}"),
        format!("http://ex.org/.well-known/genid/abcde-{random}"),
        format!("http://ex.org/.well-known/genid/abcdef0-{random}"),
        format!("http://ex.org/.well-known/genid/abcdef-{random}0"),
        format!("http://ex.org/.well-known/genid/abcdef-{}", &random[1..]),
        format!("http://ex.org/.well-known/genid/abcdef_{random}"),
        format!("http://ex.org/.well-known/genid/abcdef-{random}/"),
        format!("http://ex.org/.well-known/genid/abcdeg-{random}"),
        format!("http://ex.org/.well-known/genid/abcdef-{}g", &random[1..]),
        format!("http://ex.org/x/.well-known/genid/abcdef-{random}"),
        "http://ex.org/age".to_string(),
    ] {
        assert_eq!(names.skolem_bucket(&iri), None, "{iri}");
    }
    Ok(())
}

#[test]
fn test_a_minted_skolem_iri_is_the_seeds_and_states_its_bucket() -> TestResult {
    let names = Names::new("http://ex.org/")?;
    // SplitMix64 seeded 42 draws 0xbdd732262feb6e95 then 0x28efe333b266f103 (Python, from the published algorithm):
    // the first's low 40 bits above the second's 64.
    let mut random = RandomSource::new(42);
    let first = names.mint_skolem(Bucket::new(0x12)?, &mut random);
    assert_eq!(
        first.as_str(),
        "http://ex.org/.well-known/genid/000012-262feb6e9528efe333b266f103"
    );
    let second = names.mint_skolem(Bucket::new(0x12)?, &mut random);
    assert_ne!(first, second, "the next draw names another node");
    assert_eq!(
        names.mint_skolem(Bucket::new(0x12)?, &mut RandomSource::new(42)),
        first
    );
    assert_eq!(
        names.skolem_bucket(second.as_str()).context("a bucket")?,
        Bucket::new(0x12)?
    );
    Ok(())
}
