/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU16;

use oxrdf::NamedNode;
use ultragraphdb_core_catalog::{CatalogError, decode_declaration, encode_declaration};
use ultragraphdb_core_mapping::{
    Cardinality, Declaration, PredicateDeclaration, PredicateKind, RelationDeclaration,
    TypeDeclaration, VectorIndexSpec,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

fn ex(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

#[test]
fn test_every_declaration_round_trips_and_every_truncation_is_refused() -> TestResult {
    let declarations = vec![
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
                        .with_dimension(NonZeroU16::new(768).context("768 is not zero")?),
                )
                .with_domains(vec![ex("Person"), ex("Org")]),
        ),
        Declaration::Predicate(
            PredicateDeclaration::new(ex("undimensioned"))
                .with_vector_index(VectorIndexSpec::new("hnsw", Vec::new())),
        ),
        Declaration::Predicate(PredicateDeclaration::new(ex("e")).with_kind(PredicateKind::Edge)),
        Declaration::EntityType(TypeDeclaration::new(ex("T"), Some(ex("P")), true)),
        Declaration::EntityType(TypeDeclaration::new(ex("U"), None, false)),
        Declaration::Relation(RelationDeclaration::new(ex("R"), vec![ex("a"), ex("b")])),
    ];
    for declaration in declarations {
        let bytes = encode_declaration(&declaration)?;
        assert_eq!(decode_declaration(b"k", &bytes), Ok(declaration.clone()));
        for end in 0..bytes.len() {
            assert_eq!(
                decode_declaration(b"k", &bytes[..end]),
                Err(corrupted("the entry ends early")),
                "{declaration:?} cut at {end}"
            );
        }
        let mut longer = bytes.clone();
        longer.push(0);
        assert_eq!(
            decode_declaration(b"k", &longer),
            Err(corrupted("bytes follow the declaration"))
        );
    }
    Ok(())
}

#[test]
fn test_unknown_codes_are_refused() -> TestResult {
    let bytes = encode_declaration(&Declaration::Predicate(PredicateDeclaration::new(ex("p"))))?;
    let kind_at = 2 + 4 + "http://ex.org/p".len();
    // After the kind and cardinality bytes: the datatype's presence flag, then the unique flag.
    for (at, value, reason) in [
        (1, 9u8, "unknown entry kind 9"),
        (1, 0, "unknown entry kind 0"),
        (kind_at, 3, "unknown predicate kind 3"),
        (kind_at + 1, 2, "unknown cardinality 2"),
        (kind_at + 2, 2, "flag byte 2"),
        (kind_at + 3, 0xFF, "flag byte 255"),
    ] {
        let mut bad = bytes.clone();
        bad[at] = value;
        assert_eq!(
            decode_declaration(b"k", &bad),
            Err(corrupted(reason)),
            "byte {at} = {value}"
        );
    }
    Ok(())
}

#[test]
fn test_a_vector_index_s_dimension_is_two_big_endian_bytes_after_its_factory() -> TestResult {
    let with = |dimension: Option<u16>| -> TestResult<Vec<u8>> {
        let mut spec = VectorIndexSpec::new("hnsw", Vec::new());
        if let Some(d) = dimension {
            spec = spec.with_dimension(NonZeroU16::new(d).context("the dimension is not zero")?);
        }
        Ok(encode_declaration(&Declaration::Predicate(
            PredicateDeclaration::new(ex("p")).with_vector_index(spec),
        ))?)
    };
    // After the factory's name, a u32 big-endian length and "hnsw": 768 is 0x0300, none is 0.
    let factory_end = |bytes: &[u8]| {
        bytes
            .windows(4)
            .position(|w| w == b"hnsw")
            .map(|at| at + 4)
            .context("the factory's name")
    };
    let declared = with(Some(768))?;
    let declared_at = factory_end(&declared)?;
    assert_eq!(&declared[declared_at..declared_at + 2], &[0x03, 0x00]);
    let none = with(None)?;
    let none_at = factory_end(&none)?;
    assert_eq!(&none[none_at..none_at + 2], &[0x00, 0x00]);
    assert_eq!(declared.len(), none.len());
    Ok(())
}

#[test]
fn test_an_entry_of_the_format_before_the_dimension_is_refused() -> TestResult {
    let mut bytes =
        encode_declaration(&Declaration::Predicate(PredicateDeclaration::new(ex("p"))))?;
    assert_eq!(
        bytes[0], 2,
        "the format carrying a vector index's dimension"
    );
    bytes[0] = 1;
    assert_eq!(
        decode_declaration(b"k", &bytes),
        Err(corrupted("unknown format 1"))
    );
    Ok(())
}

/// The corruption of the entry under the key `k`, as `reason` says.
fn corrupted(reason: &str) -> CatalogError {
    CatalogError::corrupted(b"k", reason)
}

#[test]
fn test_a_string_that_is_not_utf8_or_an_iri_that_does_not_parse_is_refused() -> TestResult {
    let bytes = encode_declaration(&Declaration::Relation(RelationDeclaration::new(
        ex("R"),
        Vec::new(),
    )))?;
    // The IRI's bytes begin after the format, the kind and the 4-byte length.
    let mut not_utf8 = bytes.clone();
    not_utf8[6] = 0xFF;
    assert_eq!(
        decode_declaration(b"k", &not_utf8),
        Err(corrupted("a string is not UTF-8"))
    );
    let mut not_an_iri = bytes.clone();
    not_an_iri[6] = b' ';
    let refused = decode_declaration(b"k", &not_an_iri);
    let reason = oxrdf::NamedNode::new(" ttp://ex.org/R")
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(!reason.is_empty());
    assert_eq!(refused, Err(corrupted(&reason)));
    Ok(())
}
