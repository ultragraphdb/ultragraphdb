/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A write's reads of what it and its snapshot hold: a graph or predicate no statement can be held under finds nothing,
//! a value giving no token finds nothing, a stored blank node reads back as a blank node, and a list the write changed
//! answers a lookup by the literals it holds, its IRIs passed over.

use oxrdf::{BlankNode, GraphName, NamedNode, NamedOrBlankNode};
use ultragraphdb_store_apply::Execution;
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_store_quads::skolem_blank_node;
use ultragraphdb_store_sim::{SimListCache, SimStore};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_blocks::TestBlockSource;

use crate::utils::changesets::{
    TENANT, TestChangesets, changeset, changesets, declare, delete, ex, insert, reverse, term,
    write,
};

type TestExecution<'a> = Execution<'a, SimStore, SimListCache<CachedList>, TestBlockSource>;

fn begin(a: &TestChangesets) -> TestResult<TestExecution<'_>> {
    Ok(a.begin(TENANT, a.quads().lists().applied_ts())?)
}

/// A stale entry, which every read clears before it answers.
fn stale() -> Vec<NamedOrBlankNode> {
    vec![NamedOrBlankNode::from(ex("stale"))]
}

/// The skolem IRI the store holds the blank subject of `_:x <knows> <b>` under, written first.
fn held_skolem(a: &TestChangesets) -> TestResult<String> {
    write(a, &["_:x <knows> <b>"])?;
    let subject = *reverse(a, "knows", "b")?
        .first()
        .context("the blank subject")?;
    Ok(a.quads().iri_of(TENANT, subject)?.0)
}

/// A skolem IRI of the tenant the store does not hold: `held`, its last random digit changed.
fn unheld_skolem(a: &TestChangesets, held: &str) -> TestResult<NamedNode> {
    let (kept, last) = held.split_at(held.len() - 1);
    let unheld = format!("{kept}{}", if last == "0" { "1" } else { "0" });
    assert_eq!(a.quads().id_of(TENANT, &unheld)?, None, "{unheld}");
    Ok(NamedNode::new(unheld)?)
}

/// The subjects holding `<knows>` to `<b>` in the default graph, as `e` reads them.
fn knowing_b(e: &mut TestExecution<'_>) -> TestResult<Vec<NamedOrBlankNode>> {
    let (knows, b) = (ex("knows"), term("<b>")?);
    let mut out = stale();
    e.subjects_into(
        GraphName::DefaultGraph.as_ref(),
        knows.as_ref(),
        b.as_ref(),
        &mut out,
    )?;
    Ok(out)
}

/// The subjects of `<title>` holding `value` by its `term` index in the default graph, as `e` reads them.
fn titled(e: &mut TestExecution<'_>, value: &str) -> TestResult<Vec<NamedOrBlankNode>> {
    let (title, value) = (ex("title"), term(value)?);
    let mut out = stale();
    e.lookup_into(
        GraphName::DefaultGraph.as_ref(),
        title.as_ref(),
        "term",
        value.as_ref(),
        &mut out,
    )?;
    Ok(out)
}

#[test]
fn test_a_graph_or_predicate_no_statement_is_held_under_finds_nothing() -> TestResult {
    let a = changesets()?;
    declare(&a, "title", "with a `term` index")?;
    write(&a, &["<a> <title> \"red\""])?;
    let held = held_skolem(&a)?;
    let skolem = unheld_skolem(&a, &held)?;
    let mut e = begin(&a)?;
    let unnamed = GraphName::BlankNode(BlankNode::new("g")?);
    let (knows, title, b, red) = (ex("knows"), ex("title"), term("<b>")?, term("\"red\"")?);
    let mut out = stale();
    e.subjects_into(unnamed.as_ref(), knows.as_ref(), b.as_ref(), &mut out)?;
    assert!(out.is_empty(), "a blank graph the write did not name");
    out = stale();
    e.subjects_into(
        GraphName::DefaultGraph.as_ref(),
        skolem.as_ref(),
        b.as_ref(),
        &mut out,
    )?;
    assert!(out.is_empty(), "a skolem predicate the store does not hold");
    out = stale();
    e.lookup_into(
        unnamed.as_ref(),
        title.as_ref(),
        "term",
        red.as_ref(),
        &mut out,
    )?;
    assert!(
        out.is_empty(),
        "a lookup in a blank graph the write did not name"
    );
    assert_eq!(titled(&mut e, "\"red\"")?, vec![ex("a").into()]);
    Ok(())
}

/// A literal of a datatype the tokenizer does not take gives it no token, and finds nothing.
#[test]
fn test_a_value_giving_no_token_finds_nothing() -> TestResult {
    let a = changesets()?;
    declare(&a, "title", "with a `term` index")?;
    write(&a, &["<a> <title> \"7\"", "<b> <title> 7"])?;
    let mut e = begin(&a)?;
    assert!(
        titled(&mut e, "7")?.is_empty(),
        "an integer gives `term` no token"
    );
    assert_eq!(titled(&mut e, "\"7\"")?, vec![ex("a").into()]);
    Ok(())
}

/// A stored blank node subject reads back as the blank node its skolem IRI names.
#[test]
fn test_a_stored_blank_subject_reads_back_as_its_blank_node() -> TestResult {
    let a = changesets()?;
    let held = held_skolem(&a)?;
    let mut e = begin(&a)?;
    assert_eq!(knowing_b(&mut e)?, vec![skolem_blank_node(&held).into()]);
    Ok(())
}

/// A list the write changed answers a lookup by the literals it holds now, whatever its snapshot held; an IRI it holds
/// gives no token and is passed over.
#[test]
fn test_a_changed_list_holding_an_iri_answers_by_its_literals() -> TestResult {
    let a = changesets()?;
    declare(&a, "title", "with a `term` index")?;
    write(&a, &["<a> <title> \"red\"", "<b> <title> \"red\""])?;
    let mut e = begin(&a)?;
    e.apply(&changeset(vec![
        delete("<a> <title> \"red\"")?,
        insert("<a> <title> <x>")?,
        insert("<a> <title> \"blue\"")?,
        insert("<c> <title> <y>")?,
        insert("<c> <title> \"red fox\"")?,
    ]))?;
    let mut found = titled(&mut e, "\"red\"")?;
    found.sort_by_key(ToString::to_string);
    let mut expected: Vec<NamedOrBlankNode> = vec![ex("b").into(), ex("c").into()];
    expected.sort_by_key(ToString::to_string);
    assert_eq!(found, expected);
    Ok(())
}
