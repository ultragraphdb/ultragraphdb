/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;

use oxrdf::vocab::{rdf, xsd};
use oxrdf::{GraphName, Literal, Quad};
use ultragraphdb_core_mapping::Cardinality;
use ultragraphdb_store_quads::{DataDictionary, Shapes};

use crate::utils::helpers::{ex, node, quad, quads, write};
use ultragraphdb_tool_bdd::TestResult;

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
