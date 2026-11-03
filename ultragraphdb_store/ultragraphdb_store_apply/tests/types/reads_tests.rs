/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What a changeset's execution records as read: an insert tests its statement, deletes, overwrites, unique checks and
//! counts read lists whole, terms new to the store are recorded absent by ID and no dictionary key is, a declaration checking its
//! predicate's data records the ranges it scanned, and the namespace's catalog range is always there.

use oxrdf::{GraphName, Literal, Term};
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{DEFAULT_GRAPH, Graph, Keyspace, QuadKey};
use ultragraphdb_core_mapping::{Change, Changeset, Declaration, PredicateDeclaration};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_apply::Reads;

use crate::utils::changesets::{
    NS, TestChangesets, apply, changesets, declare, delete, ex, graph, insert, uid, write,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

/// The forward key of `<subject>`'s `<predicate>` in graph `g`.
fn forward(a: &TestChangesets, g: Graph, predicate: &str, subject: &str) -> TestResult<Vec<u8>> {
    Ok(QuadKey::Forward {
        namespace: NS,
        graph: g,
        predicate: uid(a, predicate)?,
        subject: uid(a, subject)?,
    }
    .to_bytes())
}

/// The encoding of the IRI `<name>` as an object.
fn object(a: &TestChangesets, name: &str) -> TestResult<Vec<u8>> {
    Ok(EncodedTerm::id(uid(a, name)?).as_bytes().to_vec())
}

fn reads_of(a: &TestChangesets, changes: Vec<Change>) -> TestResult<Reads> {
    Ok(apply(a, changes)
        .context("the changeset applies")?
        .reads()
        .clone())
}

/// Whether `key` lies in one of `reads`' ranges.
/// Whether no list, statement or range `reads` records is a dictionary key.
fn no_dictionary_key(reads: &Reads) -> bool {
    let dictionary = |key: &[u8]| key.first() == Some(&Keyspace::Dictionary.byte());
    !reads.lists().iter().any(|k| dictionary(k))
        && !reads.statements().iter().any(|(k, _)| dictionary(k))
        && !reads.ranges().iter().any(|(start, _)| dictionary(start))
}

fn in_a_range(reads: &Reads, key: &[u8]) -> bool {
    reads.ranges().iter().any(|(start, end)| {
        key >= start.as_slice() && end.as_ref().is_none_or(|e| key < e.as_slice())
    })
}

#[test]
fn test_an_insert_records_the_statement_it_tested_and_not_the_list() -> TestResult {
    let a = changesets()?;
    write(&a, &["<a> <knows> <c>", "<b> <p> <q>"])?;
    let reads = reads_of(&a, vec![insert("<a> <knows> <b>")?])?;
    let key = forward(&a, DEFAULT_GRAPH, "knows", "a")?;
    assert_eq!(reads.statements(), &[(key.clone(), object(&a, "b")?)]);
    assert!(!reads.lists().contains(&key), "{:?}", reads.lists());
    assert!(reads.absent().is_empty(), "every term was known");
    Ok(())
}

#[test]
fn test_an_insert_of_a_held_statement_records_its_test() -> TestResult {
    let a = changesets()?;
    write(&a, &["<a> <knows> <b>"])?;
    let reads = reads_of(&a, vec![insert("<a> <knows> <b>")?])?;
    let key = forward(&a, DEFAULT_GRAPH, "knows", "a")?;
    assert_eq!(reads.statements(), &[(key.clone(), object(&a, "b")?)]);
    assert!(!reads.lists().contains(&key));
    Ok(())
}

#[test]
fn test_an_overwrite_of_a_single_valued_predicate_reads_the_list_whole() -> TestResult {
    let a = changesets()?;
    declare(&a, "name", "single-valued")?;
    write(&a, &[r#"<a> <name> "A""#])?;
    let reads = reads_of(&a, vec![insert(r#"<a> <name> "B""#)?])?;
    assert!(
        reads
            .lists()
            .contains(&forward(&a, DEFAULT_GRAPH, "name", "a")?)
    );
    Ok(())
}

#[test]
fn test_a_unique_check_reads_the_objects_reverse_list_whole() -> TestResult {
    let a = changesets()?;
    declare(&a, "id", "unique")?;
    write(&a, &[r#"<b> <id> "y""#])?;
    let reads = reads_of(&a, vec![insert(r#"<a> <id> "x""#)?])?;
    let id = uid(&a, "id")?;
    assert!(
        reads.lists().iter().any(|k| matches!(
            QuadKey::parse(k),
            Ok((QuadKey::Reverse { predicate, .. }, None)) if predicate == id
        )),
        "{:?}",
        reads.lists()
    );
    Ok(())
}

#[test]
fn test_a_count_index_reads_the_forward_and_reverse_lists_whole() -> TestResult {
    let a = changesets()?;
    declare(&a, "friend", "with a count index")?;
    write(&a, &["<a> <friend> <c>", "<b> <p> <q>"])?;
    let reads = reads_of(&a, vec![insert("<a> <friend> <b>")?])?;
    let reverse = QuadKey::Reverse {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        predicate: uid(&a, "friend")?,
        object: EncodedTerm::id(uid(&a, "b")?),
    }
    .to_bytes();
    assert!(
        reads
            .lists()
            .contains(&forward(&a, DEFAULT_GRAPH, "friend", "a")?)
    );
    assert!(reads.lists().contains(&reverse));
    Ok(())
}

#[test]
fn test_a_delete_reads_the_subjects_list_whole() -> TestResult {
    let a = changesets()?;
    write(&a, &["<a> <knows> <b>"])?;
    let reads = reads_of(&a, vec![delete("<a> <knows> <b>")?])?;
    assert!(
        reads
            .lists()
            .contains(&forward(&a, DEFAULT_GRAPH, "knows", "a")?)
    );
    Ok(())
}

#[test]
fn test_terms_new_to_the_store_are_recorded_absent() -> TestResult {
    let a = changesets()?;
    write(&a, &["<a> <knows> <b>"])?;
    let reads = reads_of(&a, vec![insert("<n1> <knows> <n2>")?])?;
    let mut expected = vec![uid(&a, "n1")?, uid(&a, "n2")?];
    expected.sort();
    assert_eq!(reads.absent(), expected.as_slice());
    assert!(no_dictionary_key(&reads), "no dictionary key is read");
    Ok(())
}

#[test]
fn test_a_delete_naming_a_term_the_store_lacks_records_it_absent() -> TestResult {
    let a = changesets()?;
    write(&a, &["<a> <knows> <b>"])?;
    let reads = reads_of(&a, vec![delete("<a> <knows> <missing>")?])?;
    assert_eq!(reads.absent(), &[uid(&a, "missing")?]);
    assert!(no_dictionary_key(&reads), "no dictionary key is read");
    // The delete reads its subject's list whole, as a delete does.
    assert_eq!(reads.lists(), &[forward(&a, DEFAULT_GRAPH, "knows", "a")?]);
    assert!(reads.statements().is_empty());
    Ok(())
}

/// A declaration narrowing a predicate holding data scans its forward keys in every graph: the ranges recorded hold
/// every forward key of the predicate, in a graph holding it, in a graph holding only another predicate, in a graph
/// between two that hold keys, and past the last, and no key of another namespace.
#[test]
fn test_a_declaration_checking_its_data_records_the_ranges_it_scanned() -> TestResult {
    let a = changesets()?;
    // `early` takes its UID before `nick`, so in graph `h` its keys come first and `nick`'s lie past the first key
    // the skip scan finds there.
    write(
        &a,
        &[
            "<z> <early> <q>",
            r#"<a> <nick> "x""#,
            r#"<a> <nick> "y" <g>"#,
            "<b> <early> <q> <h>",
        ],
    )?;
    assert!(uid(&a, "early")? < uid(&a, "nick")?);
    let reads = reads_of(
        &a,
        vec![Change::Declare(Declaration::Predicate(
            PredicateDeclaration::new(ex("nick"))
                .with_cardinality(ultragraphdb_core_mapping::Cardinality::Single),
        ))],
    )?;
    let (g, h) = (graph(&a, "g")?, graph(&a, "h")?);
    let nick = uid(&a, "nick")?;
    // A forward key of `nick` from its layout, its graph and subject any 16 bytes: a key a write could hold there.
    let key_in = |graph: u128, subject: u128, namespace: u64| {
        let mut key = vec![Keyspace::Forward.byte()];
        key.extend_from_slice(&namespace.to_be_bytes());
        key.extend_from_slice(&graph.to_be_bytes());
        key.extend_from_slice(&nick.to_be_bytes());
        key.extend_from_slice(&subject.to_be_bytes());
        key
    };
    let named = |graph: Graph| u128::from_be_bytes(graph.to_be_bytes());
    let (g, h) = (named(g), named(h));
    let subject = uid(&a, "a")?.to_u128();
    for graph in [0, 1, g - 1, g, g + 1, h - 1, h, h + 1, u128::MAX] {
        for s in [1, subject, u128::MAX] {
            assert!(
                in_a_range(&reads, &key_in(graph, s, NS)),
                "graph {graph:x}, subject {s:x}: {:?}",
                reads.ranges()
            );
        }
    }
    assert!(!in_a_range(&reads, &key_in(0, subject, NS + 1)));
    Ok(())
}

/// A declaration that neither narrows its predicate nor adds an index reads none of its data.
#[test]
fn test_a_declaration_without_a_data_check_records_no_range() -> TestResult {
    let a = changesets()?;
    write(&a, &[r#"<a> <nick> "x""#])?;
    let reads = reads_of(
        &a,
        vec![Change::Declare(Declaration::Predicate(
            PredicateDeclaration::new(ex("nick")),
        ))],
    )?;
    assert!(reads.ranges().is_empty(), "{:?}", reads.ranges());
    Ok(())
}

/// The catalog range is the namespace's catalog keys: the catalog keyspace byte, then the namespace, to the next
/// namespace. Namespace 255 checks the carry: its end is namespace 256's prefix.
#[test]
fn test_the_catalog_range_is_the_namespaces() -> TestResult {
    let a = changesets()?;
    let reads = reads_of(&a, vec![insert("<a> <knows> <b>")?])?;
    assert_eq!(reads.namespace(), NS);
    assert_eq!(
        reads.catalog(),
        (
            vec![0x01, 0, 0, 0, 0, 0, 0, 0, 0],
            Some(vec![0x01, 0, 0, 0, 0, 0, 0, 0, 1])
        )
    );
    let other = Changeset::new(255).with(Change::Insert {
        graph: GraphName::DefaultGraph,
        subject: ex("a").into(),
        predicate: ex("knows"),
        object: Term::from(Literal::new_simple_literal("v")),
    });
    let other_reads = a.apply(&other).context("applies")?.reads().clone();
    assert_eq!(other_reads.namespace(), 255);
    assert_eq!(
        other_reads.catalog(),
        (
            vec![0x01, 0, 0, 0, 0, 0, 0, 0, 0xFF],
            Some(vec![0x01, 0, 0, 0, 0, 0, 0, 1])
        )
    );
    Ok(())
}

/// An empty changeset reads nothing but its catalog range once its namespace has a catalog; the first changeset of a
/// namespace looks up `rdf:type` for the initial catalog, and finds it absent on a new store.
#[test]
fn test_an_empty_changeset_reads_nothing() -> TestResult {
    let a = changesets()?;
    let first = reads_of(&a, Vec::new())?;
    let rdf_type: Option<SystemId> = a.quads().id_of(NS, oxrdf::vocab::rdf::TYPE.as_str())?;
    assert_eq!(first.absent(), rdf_type.as_slice());
    assert!(no_dictionary_key(&first));
    let reads = reads_of(&a, Vec::new())?;
    assert!(reads.lists().is_empty());
    assert!(reads.statements().is_empty());
    assert!(reads.ranges().is_empty());
    assert!(reads.absent().is_empty());
    Ok(())
}
