/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU16;

use oxrdf::NamedNode;
use oxrdf::vocab::xsd;
use ultragraphdb_core_mapping::{
    Cardinality, PredicateDeclaration, PredicateKind, VectorIndexSpec,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

fn iri(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

#[test]
fn test_an_undeclared_predicate_is_either_kind_multi_valued_unconstrained_and_unindexed() {
    // The defaults `PredicateDeclaration::new` documents: either kind, multi-valued, no constraint, values not unique,
    // no index, declared on no type.
    let d = PredicateDeclaration::new(iri("name"));
    assert_eq!(d.predicate(), &iri("name"));
    assert_eq!(d.kind(), PredicateKind::Either);
    assert_eq!(d.cardinality(), Cardinality::Multi);
    assert_eq!(d.datatype(), None);
    assert!(!d.unique());
    assert!(d.tokenizers().is_empty());
    assert!(!d.count_index());
    assert_eq!(d.vector_index(), None);
    assert!(d.domains().is_empty());
}

#[test]
fn test_every_builder_sets_exactly_what_its_getter_reads_back() -> TestResult {
    // Each value differs from its default above, so a getter that returns the default reads back the wrong value.
    let spec = VectorIndexSpec::new(
        "hnsw",
        vec![("metric".to_string(), "euclidean".to_string())],
    )
    .with_dimension(NonZeroU16::new(3).context("3 is not zero")?);
    let d = PredicateDeclaration::new(iri("age"))
        .with_kind(PredicateKind::Property)
        .with_cardinality(Cardinality::Single)
        .with_datatype(xsd::INTEGER.into_owned())
        .with_unique(true)
        .with_tokenizers(vec!["int".to_string(), "exact".to_string()])
        .with_count_index(true)
        .with_vector_index(spec.clone())
        .with_domains(vec![iri("Person"), iri("Robot")]);
    assert_eq!(d.kind(), PredicateKind::Property);
    assert_eq!(d.cardinality(), Cardinality::Single);
    assert_eq!(d.datatype(), Some(&xsd::INTEGER.into_owned()));
    assert!(d.unique());
    assert_eq!(d.tokenizers(), ["int".to_string(), "exact".to_string()]);
    assert!(d.count_index());
    assert_eq!(d.vector_index(), Some(&spec));
    assert_eq!(d.domains(), [iri("Person"), iri("Robot")]);

    // The kind and cardinality not the defaults' neighbours either: Edge is the first variant, Either the default.
    let e = PredicateDeclaration::new(iri("knows")).with_kind(PredicateKind::Edge);
    assert_eq!(e.kind(), PredicateKind::Edge);

    // Removing the vector index reads back none.
    assert_eq!(d.without_vector_index().vector_index(), None);
    Ok(())
}
