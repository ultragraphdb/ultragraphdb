/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The data-dictionary scenarios: statements written as text, `<a> <name> "Ann"`, its IRIs under `http://ex.org/`.

use std::sync::Arc;

use cucumber::{given, then, when};
use oxrdf::vocab::{rdf, xsd};
use oxrdf::{GraphName, Literal, Quad, Term};
use ultragraphdb_core_catalog::Catalog;
use ultragraphdb_core_mapping::{Cardinality, Declaration, PredicateDeclaration, PredicateKind};
use ultragraphdb_store_quads::{DataDictionary, Description};

use crate::Steps;
use crate::utils::helpers::{TestQuads, commit, ex, insert_write, quads, write};

fn quoted(text: &str) -> Vec<String> {
    text.split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

fn term(text: &str) -> Term {
    if text == "rdf:type" {
        return rdf::TYPE.into_owned().into();
    }
    if let Some(name) = text.strip_prefix('<').and_then(|t| t.strip_suffix('>')) {
        return ex(name).into();
    }
    Literal::new_simple_literal(text.trim_matches('"')).into()
}

/// The quad a statement's text names, in the default graph.
fn statement(text: &str) -> Quad {
    let (s, rest) = text.split_once(' ').expect("a subject");
    let (p, o) = rest.split_once(' ').expect("a predicate");
    let named = |t: Term| match t {
        Term::NamedNode(n) => n,
        other => panic!("{other} is no IRI"),
    };
    Quad::new(
        named(term(s)),
        named(term(p)),
        term(o),
        GraphName::DefaultGraph,
    )
}

fn store(w: &mut Steps) -> &TestQuads {
    w.store.get_or_insert_with(quads)
}

fn dictionary(w: &mut Steps) -> Arc<DataDictionary> {
    w.dictionary
        .get_or_insert_with(|| Arc::new(DataDictionary::new(0)))
        .clone()
}

fn describe(w: &mut Steps) -> Description {
    let d = dictionary(w);
    let store = store(w);
    d.describe(store, store.lists().latest_ts())
        .expect("the dictionary answers")
}

fn pass(w: &mut Steps) -> u64 {
    let d = dictionary(w);
    let store = store(w);
    let ts = store.lists().latest_ts();
    d.run_pass(store, ts).expect("the pass completes");
    ts
}

#[given(regex = r"^(`.+`) (?:is|are) held$")]
fn held(w: &mut Steps, statements: String) {
    let quads: Vec<Quad> = quoted(&statements).iter().map(|t| statement(t)).collect();
    write(store(w), 0, &quads);
}

#[given("the shapes are current")]
fn shapes_current(w: &mut Steps) {
    pass(w);
}

#[given(regex = r"^`(.+)` is held and the shapes are current$")]
fn held_and_current(w: &mut Steps, text: String) {
    write(store(w), 0, &[statement(&text)]);
    pass(w);
}

#[given(
    regex = r"^`(\w+)` is declared a single-valued property of `(\w+)` with datatype `xsd:string`$"
)]
fn declared(w: &mut Steps, predicate: String, domain: String) {
    let declaration = PredicateDeclaration::new(ex(&predicate))
        .with_kind(PredicateKind::Property)
        .with_cardinality(Cardinality::Single)
        .with_datatype(xsd::STRING.into_owned())
        .with_domains(vec![ex(&domain)]);
    let store = store(w);
    // One write assigns the predicate its UID and commits the declaration under it.
    let mut write = insert_write(store, 0, &[]);
    let (terms, q) = write.terms();
    let p = terms
        .iri(q.store(), q.leases(), ex(&predicate).as_str())
        .expect("a UID");
    write.request().add_entry(Catalog::entry(
        0,
        p,
        &Declaration::Predicate(declaration),
        0,
    ));
    commit(store, vec![write])
        .remove(0)
        .expect("the declaration commits");
}

#[when(regex = r"^`(.+)` is written and the next pass completes$")]
fn written_then_pass(w: &mut Steps, text: String) {
    write(store(w), 0, &[statement(&text)]);
    let ts = pass(w);
    w.ts.push(ts);
}

#[when(regex = r"^`(.+)` is written while a pass over the snapshot before it runs$")]
fn written_during_pass(w: &mut Steps, text: String) {
    let d = dictionary(w);
    let store = store(w);
    let before = store.lists().latest_ts();
    let during = d.run_pass(store, before).expect("the pass completes");
    let write = insert_write(store, 0, &[statement(&text)]);
    let (request, terms) = write.finish();
    let read_nothing = request.read_keys().next().is_none();
    let mut batch = store.lists().batch();
    batch.push(request);
    store
        .commit(batch, &[terms])
        .expect("the batch is written")
        .remove(0)
        .expect("commits");
    let after = d.run_pass(store, before).expect("the pass completes");
    w.flags = vec![read_nothing, during == after];
}

#[then(regex = r"^the dictionary lists the label `(\w+)`$")]
fn lists_label(w: &mut Steps, label: String) {
    assert!(describe(w).labels().contains(ex(&label).as_str()));
}

#[then(
    regex = r"^the dictionary lists the property `(\w+)` of `(\w+)` with datatype `xsd:string`$"
)]
fn lists_property(w: &mut Steps, property: String, label: String) {
    let d = describe(w);
    assert!(d.property_keys().contains(ex(&property).as_str()));
    let shape = &d.shapes()[ex(&label).as_str()].properties()[ex(&property).as_str()];
    assert_eq!(shape.kinds(), &[xsd::STRING.as_str().to_string()].into());
}

#[then(regex = r"^the dictionary lists the relationship type `(\w+)` from `(\w+)` to `(\w+)`$")]
fn lists_relationship(w: &mut Steps, relationship: String, from: String, to: String) {
    let d = describe(w);
    assert!(d.relationship_types().contains(ex(&relationship).as_str()));
    let targets = &d.shapes()[ex(&from).as_str()].relationships()[ex(&relationship).as_str()];
    assert_eq!(targets, &[ex(&to).as_str().to_string()].into());
}

#[then(
    regex = r"^the dictionary reports `(\w+)` on `(\w+)`, single-valued, of datatype `xsd:string`$"
)]
fn reports_declared(w: &mut Steps, property: String, label: String) {
    let d = describe(w);
    assert!(d.labels().contains(ex(&label).as_str()));
    let shape = &d.shapes()[ex(&label).as_str()].properties()[ex(&property).as_str()];
    assert_eq!(shape.cardinality(), Cardinality::Single);
    assert_eq!(shape.kinds(), &[xsd::STRING.as_str().to_string()].into());
    assert!(
        d.predicates()
            .iter()
            .any(|p| p.predicate() == &ex(&property))
    );
}

#[then(regex = r"^the dictionary lists `(\w+)` on `(\w+)`, with the pass's snapshot timestamp$")]
fn lists_after_pass(w: &mut Steps, property: String, label: String) {
    let ts = *w.ts.last().expect("a pass");
    let d = describe(w);
    assert!(
        d.shapes()[ex(&label).as_str()]
            .properties()
            .contains_key(ex(&property).as_str())
    );
    assert_eq!(d.shapes_ts(), Some(ts));
}

#[then("the write reads no list")]
fn write_reads_nothing(w: &mut Steps) {
    assert!(w.flags[0]);
}

#[then("the pass's shapes equal those of a pass over the same snapshot after the write")]
fn pass_independent(w: &mut Steps) {
    assert!(w.flags[1]);
}
