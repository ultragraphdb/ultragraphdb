/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU16;

use oxrdf::NamedNode;
use ultragraphdb_core_catalog::{CatalogContents, CatalogErrorEnum, encode_declaration};
use ultragraphdb_core_mapping::{
    Cardinality, Declaration, PredicateDeclaration, PredicateKind, RelationDeclaration,
    TypeDeclaration, VectorIndexSpec,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

fn ex(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

/// One declaration of each kind, a predicate with every field set among them.
fn every_kind() -> TestResult<Vec<Declaration>> {
    Ok(vec![
        Declaration::Predicate(PredicateDeclaration::new(ex("plain"))),
        Declaration::Predicate(
            PredicateDeclaration::new(ex("full"))
                .with_kind(PredicateKind::Property)
                .with_cardinality(Cardinality::Single)
                .with_datatype(ex("dt"))
                .with_unique(true)
                .with_tokenizers(vec!["exact".into(), "term".into()])
                .with_count_index(true)
                .with_vector_index(
                    VectorIndexSpec::new("hnsw", vec![("metric".into(), "cosine".into())])
                        .with_dimension(NonZeroU16::new(4).context("4 is not zero")?),
                )
                .with_domains(vec![ex("Person")]),
        ),
        Declaration::EntityType(TypeDeclaration::new(
            ex("Employee"),
            Some(ex("Person")),
            true,
        )),
        Declaration::Relation(RelationDeclaration::new(
            ex("marriage"),
            vec![ex("a"), ex("b")],
        )),
    ])
}

/// The reason a refusal names, and that it names the tenant and version, or why it is not such a refusal.
fn corruption(
    result: Result<CatalogContents, ultragraphdb_core_catalog::CatalogError>,
) -> TestResult<(u64, u64, String)> {
    match result {
        Err(e) => match e.0.clone() {
            CatalogErrorEnum::CorruptedVersion {
                tenant,
                version,
                reason,
            } => Ok((tenant, version, reason)),
            CatalogErrorEnum::LiteralOnEdge { .. }
            | CatalogErrorEnum::ResourceOnProperty { .. }
            | CatalogErrorEnum::DatatypeMismatch { .. }
            | CatalogErrorEnum::UnknownTokenizer { .. }
            | CatalogErrorEnum::TokenizerDatatype { .. }
            | CatalogErrorEnum::UnknownVectorIndex { .. }
            | CatalogErrorEnum::SecondParent { .. }
            | CatalogErrorEnum::ParentDropped { .. }
            | CatalogErrorEnum::ParentCycle { .. }
            | CatalogErrorEnum::Store(_)
            | CatalogErrorEnum::TooLarge { .. } => {
                Err(TestError::new(format!("another error: {e:?}")))
            }
        },
        Ok(read) => Err(TestError::new(format!("decoded: {read:?}"))),
    }
}

#[test]
fn test_contents_round_trip() -> TestResult {
    for declarations in [Vec::new(), every_kind()?] {
        let contents = CatalogContents::new(5, 9, declarations);
        let bytes = contents.encode()?;
        assert_eq!(CatalogContents::decode(5, 9, &bytes)?, contents);
    }
    Ok(())
}

#[test]
fn test_contents_at_the_bounds_of_their_numbers_round_trip() -> TestResult {
    let contents = CatalogContents::new(u64::MAX, u64::MAX, every_kind()?);
    let bytes = contents.encode()?;
    assert_eq!(
        CatalogContents::decode(u64::MAX, u64::MAX, &bytes)?,
        contents
    );
    Ok(())
}

#[test]
fn test_the_layout_is_the_documented_one() -> TestResult {
    let declaration = Declaration::Predicate(PredicateDeclaration::new(ex("p")));
    let entry = encode_declaration(&declaration);
    let bytes = CatalogContents::new(0x0102, 0x0304, vec![declaration]).encode()?;
    let mut expected = vec![1];
    expected.extend_from_slice(&0x0102u64.to_be_bytes());
    expected.extend_from_slice(&0x0304u64.to_be_bytes());
    expected.extend_from_slice(&1u32.to_be_bytes());
    expected.extend_from_slice(&u32::try_from(entry.len())?.to_be_bytes());
    expected.extend_from_slice(&entry);
    assert_eq!(bytes, expected);
    Ok(())
}

#[test]
fn test_every_truncation_is_refused_naming_the_version() -> TestResult {
    let bytes = CatalogContents::new(2, 3, every_kind()?).encode()?;
    for len in 0..bytes.len() {
        let prefix = bytes.get(..len).context("a prefix")?;
        let (tenant, version, reason) = corruption(CatalogContents::decode(2, 3, prefix))?;
        assert_eq!((tenant, version), (2, 3), "truncated to {len}");
        // A cut inside a declaration leaves its length past the bytes, so every cut ends the contents early.
        assert_eq!(reason, "the contents end early", "truncated to {len}");
    }
    Ok(())
}

#[test]
fn test_a_byte_after_the_last_declaration_is_refused() -> TestResult {
    let mut bytes = CatalogContents::new(0, 1, every_kind()?).encode()?;
    bytes.push(0);
    let (_, _, reason) = corruption(CatalogContents::decode(0, 1, &bytes))?;
    assert_eq!(reason, "bytes follow the last declaration");
    Ok(())
}

#[test]
fn test_an_unknown_format_is_refused() -> TestResult {
    let mut bytes = CatalogContents::new(0, 1, Vec::new()).encode()?;
    for format in [0u8, 2, 255] {
        if let Some(first) = bytes.first_mut() {
            *first = format;
        }
        let (_, _, reason) = corruption(CatalogContents::decode(0, 1, &bytes))?;
        assert_eq!(reason, format!("unknown format {format}"));
    }
    Ok(())
}

#[test]
fn test_contents_naming_another_tenant_or_version_are_refused() -> TestResult {
    let bytes = CatalogContents::new(4, 6, Vec::new()).encode()?;
    let (tenant, version, reason) = corruption(CatalogContents::decode(5, 6, &bytes))?;
    assert_eq!((tenant, version), (5, 6));
    assert_eq!(reason, "the contents name tenant 4");
    let (tenant_7, version_7, other) = corruption(CatalogContents::decode(4, 7, &bytes))?;
    assert_eq!((tenant_7, version_7), (4, 7));
    assert_eq!(other, "the contents name version 6");
    Ok(())
}

#[test]
fn test_a_declaration_that_does_not_decode_is_refused() -> TestResult {
    let mut bytes = CatalogContents::new(
        0,
        1,
        vec![Declaration::Predicate(PredicateDeclaration::new(ex("p")))],
    )
    .encode()?;
    // The declaration's format byte, after the 21-byte header and its 4-byte length.
    let format = bytes.get_mut(25).context("the declaration's first byte")?;
    *format = 9;
    let (_, _, reason) = corruption(CatalogContents::decode(0, 1, &bytes))?;
    // The entry decoder's reason, without the empty key it names.
    assert_eq!(reason, "unknown format 9");
    Ok(())
}

#[test]
fn test_a_declaration_of_no_bytes_is_refused() -> TestResult {
    let mut bytes = vec![1];
    bytes.extend_from_slice(&0u64.to_be_bytes());
    bytes.extend_from_slice(&1u64.to_be_bytes());
    bytes.extend_from_slice(&1u32.to_be_bytes());
    bytes.extend_from_slice(&0u32.to_be_bytes());
    let (_, _, reason) = corruption(CatalogContents::decode(0, 1, &bytes))?;
    assert_eq!(reason, "the entry ends early");
    Ok(())
}

#[test]
fn test_a_count_past_the_bytes_is_refused_at_once() -> TestResult {
    let mut bytes = vec![1];
    bytes.extend_from_slice(&0u64.to_be_bytes());
    bytes.extend_from_slice(&1u64.to_be_bytes());
    bytes.extend_from_slice(&u32::MAX.to_be_bytes());
    let (_, _, reason) = corruption(CatalogContents::decode(0, 1, &bytes))?;
    assert_eq!(reason, "the contents end early");
    Ok(())
}

#[test]
fn test_a_declaration_length_past_the_end_is_refused() -> TestResult {
    let mut bytes = CatalogContents::new(
        0,
        1,
        vec![Declaration::Predicate(PredicateDeclaration::new(ex("p")))],
    )
    .encode()?;
    let length = bytes.get_mut(21..25).context("the declaration's length")?;
    length.copy_from_slice(&u32::MAX.to_be_bytes());
    let (_, _, reason) = corruption(CatalogContents::decode(0, 1, &bytes))?;
    assert_eq!(reason, "the contents end early");
    Ok(())
}

#[test]
fn test_a_list_longer_than_the_encoding_holds_is_refused() -> TestResult {
    let at_max: Vec<NamedNode> = (0..u16::MAX).map(|i| ex(&i.to_string())).collect();
    let mut past_max = at_max.clone();
    past_max.push(ex("one-more"));
    let fits = Declaration::Relation(RelationDeclaration::new(ex("r"), at_max));
    let contents = CatalogContents::new(0, 1, vec![fits]);
    assert_eq!(
        CatalogContents::decode(0, 1, &contents.encode()?)?,
        contents
    );
    let past = Declaration::Relation(RelationDeclaration::new(ex("r"), past_max));
    let refused = CatalogContents::new(0, 1, vec![past]).encode();
    match refused.map_err(|e| e.0) {
        Err(CatalogErrorEnum::TooLarge {
            iri,
            what,
            len,
            max,
        }) => {
            assert_eq!(
                (iri.as_str(), what, len, max),
                ("http://ex.org/r", "roles", 65_536, 65_535)
            );
        }
        other => {
            return Err(TestError::new(format!(
                "not refused as too large: {other:?}"
            )));
        }
    }
    Ok(())
}

/// The tenant and version the fixed part of `bytes` names, (0, 1) when it is too short to name them.
fn named(bytes: &[u8]) -> (u64, u64) {
    let read = |range: std::ops::Range<usize>| {
        bytes
            .get(range)
            .and_then(|b| <[u8; 8]>::try_from(b).ok())
            .map(u64::from_be_bytes)
    };
    (read(1..9).unwrap_or(0), read(9..17).unwrap_or(1))
}

#[test]
fn test_every_fuzz_seed_decodes_as_its_name_says() -> TestResult {
    // The seeds of `catalog_contents_decode`, encoded by testdata/seeds.py from the formats' documentation alone.
    let dir = ultragraphdb_tool_bdd::testdata_dir()?
        .join("fuzz_seeds")
        .join("catalog_contents_decode");
    let seeds = ultragraphdb_tool_bdd::fuzz_regressions_in(&dir)?;
    assert_eq!(seeds.len(), 10);
    for (name, bytes) in seeds {
        let (tenant, version) = named(&bytes);
        match CatalogContents::decode(tenant, version, &bytes) {
            Ok(contents) => {
                assert!(!name.starts_with("malformed"), "{name} accepted");
                assert!(
                    name.ends_with(&format!("tenant_{tenant}_version_{version}")),
                    "{name}"
                );
                assert_eq!(contents.encode()?, bytes, "{name}");
            }
            Err(error) => {
                assert!(name.starts_with("malformed"), "{name}: {error}");
                assert!(
                    matches!(error.0, CatalogErrorEnum::CorruptedVersion { .. }),
                    "{name}: {error}"
                );
            }
        }
    }
    Ok(())
}
