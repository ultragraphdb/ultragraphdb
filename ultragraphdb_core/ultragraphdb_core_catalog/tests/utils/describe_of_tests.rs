/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::NamedNode;
use ultragraphdb_core_catalog::describe_of;
use ultragraphdb_core_mapping::{
    Cardinality, Declaration, PredicateDeclaration, PredicateKind, TypeDeclaration, VectorIndexSpec,
};
use ultragraphdb_protocol_wire::{DeclaredKind, DeclaredPredicate};
use ultragraphdb_tool_bdd::TestResult;

fn ex(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

/// Each predicate is described with what it declares: a single-valued one as single, a multi-valued one as not, and its
/// kind, uniqueness, datatype, tokenizers, count index, domains and vector index as declared.
#[test]
fn test_a_predicate_is_described_as_declared() -> TestResult {
    let (full, plain) = (ex("full"), ex("plain"));
    let (datatype, domain) = (ex("dt"), ex("Person"));
    let tokenizers = vec!["exact".to_string(), "term".to_string()];
    let options = vec![("metric".to_string(), "cosine".to_string())];
    let declarations = [
        Declaration::Predicate(
            PredicateDeclaration::new(plain.clone()).with_kind(PredicateKind::Edge),
        ),
        Declaration::Predicate(
            PredicateDeclaration::new(full.clone())
                .with_kind(PredicateKind::Property)
                .with_cardinality(Cardinality::Single)
                .with_datatype(datatype.clone())
                .with_unique(true)
                .with_tokenizers(tokenizers.clone())
                .with_count_index(true)
                .with_vector_index(VectorIndexSpec::new("hnsw", options.clone()))
                .with_domains(vec![domain.clone()]),
        ),
    ];
    let describe = describe_of(1, 0, &declarations, std::iter::empty());
    let expected = [
        DeclaredPredicate::new(full.as_str())
            .with_kind(DeclaredKind::Property)
            .with_single(true)
            .with_unique(true)
            .with_datatype(datatype.as_str())
            .with_tokenizers(tokenizers)
            .with_count_index(true)
            .with_vector_index("hnsw", options)
            .with_domains(vec![domain.as_str().to_string()]),
        DeclaredPredicate::new(plain.as_str())
            .with_kind(DeclaredKind::Edge)
            .with_single(false),
    ];
    assert_eq!(describe.predicates(), expected.as_slice());
    Ok(())
}

/// Entity types are numbered from 1 in preorder: each type before its children, roots and siblings in their IRIs' order,
/// and listed in their IRIs' order with their numbers.
#[test]
fn test_entity_types_are_numbered_in_preorder() -> TestResult {
    // The forest: A over B and E, B over D, and C a root of its own.
    let parents = [
        ("A", None),
        ("B", Some("A")),
        ("C", None),
        ("D", Some("B")),
        ("E", Some("A")),
    ];
    let visits = ["A", "B", "D", "E", "C"];
    let declarations: Vec<Declaration> = parents
        .iter()
        .rev()
        .map(|(name, parent)| {
            Declaration::EntityType(TypeDeclaration::new(ex(name), parent.map(ex), false))
        })
        .collect();
    let describe = describe_of(1, 0, &declarations, std::iter::empty());
    let numbered: Vec<(String, u32)> = describe
        .entity_types()
        .iter()
        .map(|(held, number)| (held.iri().to_string(), *number))
        .collect();
    let expected: Vec<(String, u32)> = parents
        .iter()
        .map(|(name, _)| {
            let number = visits
                .iter()
                .zip(1_u32..)
                .find(|(visit, _)| *visit == name)
                .map_or(0, |(_, number)| number);
            (ex(name).as_str().to_string(), number)
        })
        .collect();
    assert_eq!(numbered, expected);
    Ok(())
}

/// An edge predicate is a relationship type, a property predicate a property key, and one of either kind both; a recorded
/// predicate is both only when no declaration names it, and a declared one keeps its declared kind.
#[test]
fn test_kinds_take_declared_and_recorded_predicates() -> TestResult {
    let (edge, property, either, other) = (ex("edge"), ex("property"), ex("either"), ex("other"));
    let declarations = [
        Declaration::Predicate(
            PredicateDeclaration::new(edge.clone()).with_kind(PredicateKind::Edge),
        ),
        Declaration::Predicate(
            PredicateDeclaration::new(property.clone()).with_kind(PredicateKind::Property),
        ),
        Declaration::Predicate(
            PredicateDeclaration::new(either.clone()).with_kind(PredicateKind::Either),
        ),
    ];
    let recorded = [edge.as_str(), other.as_str()];
    let describe = describe_of(1, 0, &declarations, recorded.into_iter());
    let mut edges = vec![
        edge.as_str().to_string(),
        either.as_str().to_string(),
        other.as_str().to_string(),
    ];
    edges.sort();
    let mut properties = vec![
        property.as_str().to_string(),
        either.as_str().to_string(),
        other.as_str().to_string(),
    ];
    properties.sort();
    assert_eq!(describe.relationship_types(), edges.as_slice());
    assert_eq!(describe.property_keys(), properties.as_slice());
    Ok(())
}
