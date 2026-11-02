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
use ultragraphdb_store_quads::{DataDictionary, Description, Shapes};

use crate::Steps;
use crate::utils::helpers::{TestQuads, commit, ex, insert_write, quads, write};
use ultragraphdb_tool_bdd::{Context, TestResult};

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
fn statement(text: &str) -> TestResult<Quad> {
    let (s, rest) = text.split_once(' ').context("a subject")?;
    let (p, o) = rest.split_once(' ').context("a predicate")?;
    let named = |t: Term| {
        match t {
            Term::NamedNode(n) => Ok(n),
            other @ (Term::BlankNode(_) | Term::Literal(_) | Term::Triple(_)) => {
                Err(format!("{other} is no IRI"))
            }
        }
        .context("a statement's subject and predicate are IRIs")
    };
    Ok(Quad::new(
        named(term(s))?,
        named(term(p))?,
        term(o),
        GraphName::DefaultGraph,
    ))
}

fn store(w: &mut Steps) -> TestResult<&TestQuads> {
    if w.store.is_none() {
        w.store = Some(quads()?);
    }
    w.store.as_ref().context("the store")
}

fn dictionary(w: &mut Steps) -> Arc<DataDictionary> {
    w.dictionary
        .get_or_insert_with(|| Arc::new(DataDictionary::new(0)))
        .clone()
}

fn describe(w: &mut Steps) -> TestResult<Description> {
    let d = dictionary(w);
    let store = store(w)?;
    d.describe(store, store.lists().applied_ts())
        .context("the dictionary answers")
}

/// Runs a pass at the newest timestamp: the timestamp.
fn pass_at(w: &mut Steps) -> TestResult<u64> {
    let d = dictionary(w);
    let store = store(w)?;
    let ts = store.lists().applied_ts();
    let _shapes: Arc<Shapes> = d.run_pass(store, ts).context("the pass completes")?;
    Ok(ts)
}

/// Runs a pass at the newest timestamp, for a step that does not read the timestamp.
fn pass(w: &mut Steps) -> TestResult {
    let _passed_at: u64 = pass_at(w)?;
    Ok(())
}

#[given(regex = r"^(`.+`) (?:is|are) held$")]
fn held(w: &mut Steps, args: &[String]) -> TestResult {
    let statements = &args[0];
    let quads: Vec<Quad> = quoted(statements)
        .iter()
        .map(|t| statement(t))
        .collect::<TestResult<_>>()?;
    write(store(w)?, 0, &quads)?;
    Ok(())
}

#[given("the shapes are current")]
fn shapes_current(w: &mut Steps) -> TestResult {
    pass(w)?;
    Ok(())
}

#[given(regex = r"^`(.+)` is held and the shapes are current$")]
fn held_and_current(w: &mut Steps, args: &[String]) -> TestResult {
    let text = &args[0];
    write(store(w)?, 0, &[statement(text)?])?;
    pass(w)?;
    Ok(())
}

#[given(
    regex = r"^`(\w+)` is declared a single-valued property of `(\w+)` with datatype `xsd:string`$"
)]
fn declared(w: &mut Steps, args: &[String]) -> TestResult {
    let predicate = &args[0];
    let domain = &args[1];
    let declaration = PredicateDeclaration::new(ex(predicate))
        .with_kind(PredicateKind::Property)
        .with_cardinality(Cardinality::Single)
        .with_datatype(xsd::STRING.into_owned())
        .with_domains(vec![ex(domain)]);
    let store = store(w)?;
    // One write assigns the predicate its UID and commits the declaration under it.
    let mut write = insert_write(store, 0, &[])?;
    let (terms, q) = write.terms();
    let p = terms
        .iri(q.store(), q.leases(), ex(predicate).as_str())
        .context("a UID")?;
    write.request().add_entry(Catalog::entry(
        0,
        p,
        &Declaration::Predicate(declaration),
        0,
    ));
    let _removed: u64 = commit(store, vec![write])?
        .remove(0)
        .context("the declaration commits")?;
    Ok(())
}

#[when(regex = r"^`(.+)` is written and the next pass completes$")]
fn written_then_pass(w: &mut Steps, args: &[String]) -> TestResult {
    let text = &args[0];
    write(store(w)?, 0, &[statement(text)?])?;
    let ts = pass_at(w)?;
    w.ts.push(ts);
    Ok(())
}

#[when(regex = r"^`(.+)` is written while a pass over the snapshot before it runs$")]
fn written_during_pass(w: &mut Steps, args: &[String]) -> TestResult {
    let text = &args[0];
    let d = dictionary(w);
    let store = store(w)?;
    let before = store.lists().applied_ts();
    let during = d.run_pass(store, before).context("the pass completes")?;
    let write = insert_write(store, 0, &[statement(text)?])?;
    let (request, terms) = write.finish();
    let read_nothing = request.read_keys().next().is_none();
    let mut batch = store.lists().batch();
    batch.push(request);
    let _removed: u64 = store
        .commit(batch, &[terms])
        .context("the batch is written")?
        .remove(0)
        .context("commits")?;
    let after = d.run_pass(store, before).context("the pass completes")?;
    w.flags = vec![read_nothing, during == after];
    Ok(())
}

#[then(regex = r"^the dictionary lists the label `(\w+)`$")]
fn lists_label(w: &mut Steps, args: &[String]) -> TestResult {
    let label = &args[0];
    assert!(describe(w)?.labels().contains(ex(label).as_str()));
    Ok(())
}

#[then(
    regex = r"^the dictionary lists the property `(\w+)` of `(\w+)` with datatype `xsd:string`$"
)]
fn lists_property(w: &mut Steps, args: &[String]) -> TestResult {
    let property = &args[0];
    let label = &args[1];
    let d = describe(w)?;
    assert!(d.property_keys().contains(ex(property).as_str()));
    let shape = &d.shapes()[ex(label).as_str()].properties()[ex(property).as_str()];
    assert_eq!(shape.kinds(), &[xsd::STRING.as_str().to_string()].into());
    Ok(())
}

#[then(regex = r"^the dictionary lists the relationship type `(\w+)` from `(\w+)` to `(\w+)`$")]
fn lists_relationship(w: &mut Steps, args: &[String]) -> TestResult {
    let relationship = &args[0];
    let from = &args[1];
    let to = &args[2];
    let d = describe(w)?;
    assert!(d.relationship_types().contains(ex(relationship).as_str()));
    let targets = &d.shapes()[ex(from).as_str()].relationships()[ex(relationship).as_str()];
    assert_eq!(targets, &[ex(to).as_str().to_string()].into());
    Ok(())
}

#[then(
    regex = r"^the dictionary reports `(\w+)` on `(\w+)`, single-valued, of datatype `xsd:string`$"
)]
fn reports_declared(w: &mut Steps, args: &[String]) -> TestResult {
    let property = &args[0];
    let label = &args[1];
    let d = describe(w)?;
    assert!(d.labels().contains(ex(label).as_str()));
    let shape = &d.shapes()[ex(label).as_str()].properties()[ex(property).as_str()];
    assert_eq!(shape.cardinality(), Cardinality::Single);
    assert_eq!(shape.kinds(), &[xsd::STRING.as_str().to_string()].into());
    assert!(
        d.predicates()
            .iter()
            .any(|p| p.predicate() == &ex(property))
    );
    Ok(())
}

#[then(regex = r"^the dictionary lists `(\w+)` on `(\w+)`, with the pass's snapshot timestamp$")]
fn lists_after_pass(w: &mut Steps, args: &[String]) -> TestResult {
    let property = &args[0];
    let label = &args[1];
    let ts = *w.ts.last().context("a pass")?;
    let d = describe(w)?;
    assert!(
        d.shapes()[ex(label).as_str()]
            .properties()
            .contains_key(ex(property).as_str())
    );
    assert_eq!(d.shapes_ts(), Some(ts));
    Ok(())
}

#[then("the write reads no list")]
fn write_reads_nothing(w: &mut Steps) {
    assert!(w.flags[0]);
}

#[then("the pass's shapes equal those of a pass over the same snapshot after the write")]
fn pass_independent(w: &mut Steps) {
    assert!(w.flags[1]);
}
