/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;

use oxrdf::vocab::{rdf, xsd};
use oxrdf::{GraphName, Literal, NamedNode, Quad};
use ultragraphdb_core_catalog::Catalog;
use ultragraphdb_core_mapping::{
    Cardinality, Declaration, PredicateDeclaration, PredicateKind, TypeDeclaration,
};
use ultragraphdb_store_quads::{DataDictionary, Shapes};

use crate::utils::helpers::{TestQuads, commit, ex, insert_write, node, quad, quads, write};
use ultragraphdb_tool_bdd::{Context, TestResult};

fn typed(s: &str, label: &str) -> Quad {
    Quad::new(node(s), rdf::TYPE, ex(label), GraphName::DefaultGraph)
}

#[test]
fn test_a_label_holding_two_objects_of_a_predicate_is_multi_valued() -> TestResult {
    let store = quads()?;
    write(
        &store,
        0,
        &[
            typed("a", "Person"),
            typed("b", "Person"),
            quad("a", "nick", Literal::new_simple_literal("Al")),
            quad("a", "nick", Literal::new_simple_literal("Ally")),
            quad("b", "age", Literal::new_typed_literal("30", xsd::INTEGER)),
        ],
    )?;
    let d = DataDictionary::new(0);
    let _shapes: Arc<Shapes> = d.run_pass(&store, store.lists().applied_ts())?;
    let described = d.describe(&store, store.lists().applied_ts())?;
    let person = &described.shapes()[ex("Person").as_str()];
    assert_eq!(
        person.properties()[ex("nick").as_str()].cardinality(),
        Cardinality::Multi
    );
    assert_eq!(
        person.properties()[ex("age").as_str()].cardinality(),
        Cardinality::Single
    );
    assert_eq!(
        person.properties()[ex("age").as_str()].kinds(),
        &[xsd::INTEGER.as_str().to_string()].into()
    );
    assert!(described.property_keys().contains(ex("age").as_str()));
    assert!(!described.relationship_types().contains(rdf::TYPE.as_str()));
    let statistics = &described.statistics()[&(String::new(), ex("nick").as_str().to_string())];
    assert_eq!(statistics.statements(), 2);
    Ok(())
}

#[test]
fn test_a_pass_over_an_older_snapshot_keeps_the_newer_shapes() -> TestResult {
    let store = quads()?;
    write(&store, 0, &[typed("a", "Person")])?;
    let old = store.lists().applied_ts();
    write(
        &store,
        0,
        &[quad("a", "nick", Literal::new_simple_literal("Al"))],
    )?;
    let new = store.lists().applied_ts();
    let d = DataDictionary::new(0);
    let _newer: Arc<Shapes> = d.run_pass(&store, new)?;
    let _older: Arc<Shapes> = d.run_pass(&store, old)?;
    assert_eq!(d.shapes().map(|s| s.read_ts()), Some(new));
    Ok(())
}

#[test]
fn test_no_shapes_before_the_first_pass() -> TestResult {
    let store = quads()?;
    write(&store, 0, &[typed("a", "Person")])?;
    let d = DataDictionary::new(0);
    let described = d.describe(&store, store.lists().applied_ts())?;
    assert_eq!(described.shapes_ts(), None);
    assert!(described.shapes().is_empty());
    Ok(())
}

/// Commits `declaration` under the ID one write assigns `iri`.
fn declare(store: &TestQuads, iri: &NamedNode, declaration: &Declaration) -> TestResult {
    let mut write = insert_write(store, 0, &[])?;
    let (terms, q) = write.terms();
    let uid = terms.iri(q, iri.as_str()).context("an ID")?;
    write
        .request()
        .add_entry(Catalog::entry(0, uid, declaration, 0));
    let _versions: Vec<u64> = commit(store, vec![write])?;
    Ok(())
}

/// The catalog's declarations are described without data: an edge as a relationship type, a predicate of either kind
/// as neither, an entity type as a label, and a property declared with no datatype on its domain with no kinds.
#[test]
fn test_the_catalogs_declarations_are_described_by_their_kinds() -> TestResult {
    let store = quads()?;
    let (knows, either, note, person) = (ex("knows"), ex("either"), ex("note"), ex("Person"));
    declare(
        &store,
        &knows,
        &Declaration::Predicate(
            PredicateDeclaration::new(knows.clone()).with_kind(PredicateKind::Edge),
        ),
    )?;
    declare(
        &store,
        &either,
        &Declaration::Predicate(
            PredicateDeclaration::new(either.clone()).with_kind(PredicateKind::Either),
        ),
    )?;
    declare(
        &store,
        &note,
        &Declaration::Predicate(
            PredicateDeclaration::new(note.clone())
                .with_kind(PredicateKind::Property)
                .with_domains(vec![person.clone()]),
        ),
    )?;
    let entity = TypeDeclaration::new(person.clone(), None, false);
    declare(&store, &person, &Declaration::EntityType(entity.clone()))?;
    let described = DataDictionary::new(0).describe(&store, store.lists().applied_ts())?;
    assert!(described.relationship_types().contains(knows.as_str()));
    assert!(!described.relationship_types().contains(either.as_str()));
    assert!(!described.property_keys().contains(either.as_str()));
    assert!(described.labels().contains(person.as_str()));
    assert!(
        described
            .entity_types()
            .iter()
            .any(|(declared, _)| declared == &entity)
    );
    let shape = described
        .shapes()
        .get(person.as_str())
        .context("Person's shape")?;
    let property = shape
        .properties()
        .get(note.as_str())
        .context("note on Person")?;
    assert!(property.kinds().is_empty());
    Ok(())
}
