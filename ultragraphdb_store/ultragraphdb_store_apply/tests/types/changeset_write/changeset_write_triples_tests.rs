/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Inserts and deletes of triples at their corner cases: an empty changeset, one quad twice in a changeset, a set and
//! a delete of one quad in a changeset, a subject that is its own object, the empty string, the largest UIDs, deletes
//! of what is not held, and the checks a write's object meets. Expected counts are hand-counted from each test's
//! statements.

use oxrdf::{GraphName, NamedOrBlankNode};
use ultragraphdb_core_keys::{CountDirection, DEFAULT_GRAPH, Keyspace, QuadKey};
use ultragraphdb_core_mapping::Change;
use ultragraphdb_store_apply::{Applied, ChangesetErrorEnum};
use ultragraphdb_store_quads::QuadErrorEnum;

use crate::utils::changesets::{
    NS, apply, changesets, counts_of, declare, delete, ex, exact_token, index_keys_holding,
    indexed, insert, keys_of, objects, objects_in, postings, predicate_list, reverse, sim_store,
    statistics, stored_keys, term, term_token, uid, write, write_changes,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

/// The keys the store holds that are not catalog keys.
fn quad_keys(a: &crate::utils::changesets::TestChangesets) -> Vec<Vec<u8>> {
    stored_keys(a)
        .into_iter()
        .filter(|k| !matches!(Keyspace::from_byte(k[0]), Some((Keyspace::Catalog, _))))
        .collect()
}

#[test]
fn test_an_empty_changeset_commits_and_writes_no_quad() -> TestResult {
    let a = changesets()?;
    let first = apply(&a, Vec::new()).context("an empty changeset applies")?;
    assert!(
        first.read().is_empty() && first.lists_read() == 0,
        "{first:?}"
    );
    assert!(quad_keys(&a).is_empty(), "{:?}", quad_keys(&a));
    let before = a.quads().store().contents();
    let second = apply(&a, Vec::new()).context("an empty changeset applies")?;
    assert!(
        second.written().is_empty() && second.read().is_empty(),
        "{second:?}"
    );
    assert_eq!(a.quads().store().contents().keys(), before.keys());
    Ok(())
}

#[test]
fn test_one_quad_twice_in_a_changeset_is_held_and_counted_once() -> TestResult {
    let a = changesets()?;
    declare(&a, "friend", "with an exact count index")?;
    declare(&a, "tag", "with a `term` index")?;
    write_changes(
        &a,
        vec![
            insert("<a> <friend> <b>")?,
            insert("<a> <friend> <b>")?,
            insert(r#"<a> <tag> "x""#)?,
            insert(r#"<a> <tag> "x""#)?,
        ],
    )?;
    assert_eq!(objects(&a, "a", "friend")?, vec![term("<b>")?]);
    assert_eq!(objects(&a, "a", "tag")?, vec![term(r#""x""#)?]);
    // One statement each, counted once: forward count 1 for `a`, reverse count 1 for `b`.
    assert_eq!(statistics(&a, "friend")?.statements(), 1);
    assert_eq!(statistics(&a, "tag")?.statements(), 1);
    assert_eq!(
        counts_of(&a, "friend", CountDirection::Forward, uid(&a, "a")?)?,
        vec![1]
    );
    assert_eq!(
        counts_of(&a, "friend", CountDirection::Reverse, uid(&a, "b")?)?,
        vec![1]
    );
    assert_eq!(reverse(&a, "friend", "b")?, vec![uid(&a, "a")?]);
    assert_eq!(indexed(&a, "tag", &term_token("x"))?, vec![uid(&a, "a")?]);
    Ok(())
}

#[test]
fn test_a_set_and_a_delete_of_one_quad_in_a_changeset_leave_nothing() -> TestResult {
    let a = changesets()?;
    declare(&a, "friend", "with an exact count index")?;
    declare(&a, "tag", "with a `term` index")?;
    write_changes(
        &a,
        vec![
            insert("<a> <friend> <b>")?,
            delete("<a> <friend> <b>")?,
            insert(r#"<a> <tag> "x""#)?,
            delete(r#"<a> <tag> "x""#)?,
        ],
    )?;
    assert!(objects(&a, "a", "friend")?.is_empty());
    assert!(objects(&a, "a", "tag")?.is_empty());
    assert!(reverse(&a, "friend", "b")?.is_empty());
    assert!(counts_of(&a, "friend", CountDirection::Forward, uid(&a, "a")?)?.is_empty());
    assert!(counts_of(&a, "friend", CountDirection::Reverse, uid(&a, "b")?)?.is_empty());
    assert!(index_keys_holding(&a, "tag", uid(&a, "a")?)?.is_empty());
    assert!(predicate_list(&a, "a", false)?.is_empty());
    assert!(predicate_list(&a, "b", true)?.is_empty());
    assert_eq!(statistics(&a, "friend")?.statements(), 0);
    assert_eq!(statistics(&a, "tag")?.statements(), 0);
    Ok(())
}

#[test]
fn test_a_delete_and_a_set_of_one_quad_in_a_changeset_hold_it_once() -> TestResult {
    let a = changesets()?;
    declare(&a, "friend", "with an exact count index")?;
    write_changes(&a, vec![insert("<a> <friend> <b>")?])?;
    write_changes(
        &a,
        vec![delete("<a> <friend> <b>")?, insert("<a> <friend> <b>")?],
    )?;
    assert_eq!(objects(&a, "a", "friend")?, vec![term("<b>")?]);
    assert_eq!(reverse(&a, "friend", "b")?, vec![uid(&a, "a")?]);
    assert_eq!(
        counts_of(&a, "friend", CountDirection::Forward, uid(&a, "a")?)?,
        vec![1]
    );
    assert_eq!(
        counts_of(&a, "friend", CountDirection::Reverse, uid(&a, "b")?)?,
        vec![1]
    );
    assert_eq!(statistics(&a, "friend")?.statements(), 1);
    Ok(())
}

#[test]
fn test_a_subject_that_is_its_own_object_is_kept_in_both_directions() -> TestResult {
    let a = changesets()?;
    declare(&a, "friend", "with an exact count index")?;
    write_changes(&a, vec![insert("<a> <friend> <a>")?])?;
    let (node, friend) = (uid(&a, "a")?, uid(&a, "friend")?);
    assert_eq!(reverse(&a, "friend", "a")?, vec![node]);
    assert_eq!(predicate_list(&a, "a", false)?, vec![friend]);
    assert_eq!(predicate_list(&a, "a", true)?, vec![friend]);
    // One object for the subject and one subject for the object, both the node itself.
    assert_eq!(
        counts_of(&a, "friend", CountDirection::Forward, node)?,
        vec![1]
    );
    assert_eq!(
        counts_of(&a, "friend", CountDirection::Reverse, node)?,
        vec![1]
    );
    write_changes(&a, vec![delete("<a> <friend> <a>")?])?;
    assert!(reverse(&a, "friend", "a")?.is_empty());
    assert!(predicate_list(&a, "a", false)?.is_empty());
    assert!(predicate_list(&a, "a", true)?.is_empty());
    assert!(counts_of(&a, "friend", CountDirection::Forward, node)?.is_empty());
    assert!(counts_of(&a, "friend", CountDirection::Reverse, node)?.is_empty());
    Ok(())
}

#[test]
fn test_the_empty_string_is_held_and_indexed() -> TestResult {
    let a = changesets()?;
    declare(&a, "name", "with `exact` and `term`")?;
    write_changes(&a, vec![insert(r#"<a> <name> """#)?])?;
    assert_eq!(objects(&a, "a", "name")?, vec![term(r#""""#)?]);
    // The exact token of "" is the identifier alone; Dgraph's term tokenizer gives an empty value one empty token.
    assert_eq!(indexed(&a, "name", &exact_token(""))?, vec![uid(&a, "a")?]);
    assert_eq!(indexed(&a, "name", &term_token(""))?, vec![uid(&a, "a")?]);
    write_changes(&a, vec![delete(r#"<a> <name> """#)?])?;
    assert!(objects(&a, "a", "name")?.is_empty());
    assert!(index_keys_holding(&a, "name", uid(&a, "a")?)?.is_empty());
    Ok(())
}

#[test]
fn test_a_single_valued_overwrite_moves_reverse_and_counts() -> TestResult {
    let a = changesets()?;
    declare(&a, "friend", "single-valued with an exact count index")?;
    write_changes(&a, vec![insert("<a> <friend> <b>")?])?;
    write_changes(&a, vec![insert("<a> <friend> <c>")?])?;
    let node = uid(&a, "a")?;
    assert_eq!(objects(&a, "a", "friend")?, vec![term("<c>")?]);
    assert!(reverse(&a, "friend", "b")?.is_empty());
    assert_eq!(reverse(&a, "friend", "c")?, vec![node]);
    assert_eq!(
        counts_of(&a, "friend", CountDirection::Forward, node)?,
        vec![1]
    );
    assert!(counts_of(&a, "friend", CountDirection::Reverse, uid(&a, "b")?)?.is_empty());
    assert_eq!(
        counts_of(&a, "friend", CountDirection::Reverse, uid(&a, "c")?)?,
        vec![1]
    );
    assert_eq!(statistics(&a, "friend")?.statements(), 1);
    assert!(!predicate_list(&a, "b", true)?.contains(&uid(&a, "friend")?));
    Ok(())
}

#[test]
fn test_statistics_count_each_object_kind() -> TestResult {
    let a = changesets()?;
    write_changes(
        &a,
        vec![
            insert("<a> <p> <b>")?,
            insert("<a> <p> _:z")?,
            insert(r#"<a> <p> "x""#)?,
            insert("<a> <p> 5")?,
        ],
    )?;
    let s = statistics(&a, "p")?;
    assert_eq!(s.statements(), 4);
    for (kind, n) in [
        ("iri", 1),
        ("blank", 1),
        ("http://www.w3.org/2001/XMLSchema#string", 1),
        ("http://www.w3.org/2001/XMLSchema#integer", 1),
    ] {
        assert_eq!(s.kind(kind), n, "{kind}");
    }
    write_changes(&a, vec![delete(r#"<a> <p> "x""#)?])?;
    let after_delete = statistics(&a, "p")?;
    assert_eq!(after_delete.statements(), 3);
    assert_eq!(
        after_delete.kind("http://www.w3.org/2001/XMLSchema#string"),
        0
    );
    assert_eq!(after_delete.kind("iri"), 1);
    Ok(())
}

#[test]
fn test_a_delete_of_an_object_not_held_writes_nothing() -> TestResult {
    let a = changesets()?;
    declare(&a, "name", "with a `term` index")?;
    write_changes(&a, vec![insert(r#"<a> <name> "A""#)?])?;
    let before = a.quads().store().contents();
    let applied = apply(&a, vec![delete(r#"<a> <name> "B""#)?])?;
    assert!(applied.written().is_empty(), "{:?}", applied.written());
    assert_eq!(a.quads().store().contents().keys(), before.keys());
    assert_eq!(objects(&a, "a", "name")?, vec![term(r#""A""#)?]);
    Ok(())
}

#[test]
fn test_a_delete_by_pattern_stays_in_its_graph() -> TestResult {
    let a = changesets()?;
    write_changes(
        &a,
        vec![
            insert("<a> <knows> <b> <g1>")?,
            insert("<a> <knows> <c> <g1>")?,
            insert("<a> <knows> <d>")?,
        ],
    )?;
    write_changes(
        &a,
        vec![Change::DeletePattern {
            graph: GraphName::from(ex("g1")),
            subject: NamedOrBlankNode::from(ex("a")),
            predicate: ex("knows"),
        }],
    )?;
    assert!(objects_in(&a, "g1", "a", "knows")?.is_empty());
    assert_eq!(objects(&a, "a", "knows")?, vec![term("<d>")?]);
    Ok(())
}

#[test]
fn test_a_unique_value_held_by_another_subject_is_refused_naming_both() -> TestResult {
    let a = changesets()?;
    declare(&a, "id", "unique")?;
    write_changes(&a, vec![insert(r#"<a> <id> "x""#)?])?;
    let refused = apply(&a, vec![insert(r#"<b> <id> "x""#)?])
        .err()
        .context("the error of apply(..)")?;
    assert!(
        matches!(refused.0, ChangesetErrorEnum::Unique { .. }),
        "{refused:?}"
    );
    assert_eq!(
        refused.to_string(),
        r#"Predicate <http://ex.org/id> holds unique values, and <http://ex.org/a> already holds "x""#
    );
    // Another value, and the same value in another graph, are written.
    write_changes(&a, vec![insert(r#"<b> <id> "y""#)?])?;
    write_changes(&a, vec![insert(r#"<b> <id> "x" <g1>"#)?])?;
    assert_eq!(objects(&a, "b", "id")?, vec![term(r#""y""#)?]);
    Ok(())
}

#[test]
fn test_an_object_of_the_wrong_kind_or_datatype_is_refused_with_its_text() -> TestResult {
    let a = changesets()?;
    declare(&a, "name", "a property")?;
    declare(&a, "age", "with datatype `xsd:integer`")?;
    let refused = apply(&a, vec![insert("<a> <name> <b>")?])
        .err()
        .context("the error of apply(..)")?;
    assert_eq!(
        refused.to_string(),
        "Predicate <http://ex.org/name> is declared a property and cannot hold <http://ex.org/b>"
    );
    let age_refused = apply(&a, vec![insert(r#"<a> <age> "old""#)?])
        .err()
        .context("the error of apply(..)")?;
    assert_eq!(
        age_refused.to_string(),
        "Predicate <http://ex.org/age> holds literals of <http://www.w3.org/2001/XMLSchema#integer>, not of <http://www.w3.org/2001/XMLSchema#string>"
    );
    assert!(keys_of(&a, Keyspace::Forward, "name")?.is_empty());
    assert!(keys_of(&a, Keyspace::Forward, "age")?.is_empty());
    Ok(())
}

#[test]
fn test_the_largest_sequences_are_written_and_the_next_is_refused() -> TestResult {
    // One block of the Resource type, the last four sequences, 2^64 - 5 to 2^64 - 2, and then none.
    let last = ultragraphdb_traits_blocks::SequenceBlock::new(u64::MAX - 4, u64::MAX)
        .context("a block")?;
    let source = crate::utils::changesets::blocks()
        .with_blocks(ultragraphdb_core_ids::TypePrefix::RESOURCE, vec![last]);
    let a = crate::utils::changesets::changesets_with(sim_store(), source, 1)?;
    // rdf:type, then <a>, <knows> and <b>, in the order the write resolves them.
    write_changes(&a, vec![insert("<a> <knows> <b>")?])?;
    let sequence = |name: &str| -> TestResult<u64> { Ok(uid(&a, name)?.sequence().get()) };
    assert_eq!(
        (sequence("a")?, sequence("knows")?, sequence("b")?),
        (u64::MAX - 3, u64::MAX - 2, u64::MAX - 1)
    );
    assert_eq!(reverse(&a, "knows", "b")?, vec![uid(&a, "a")?]);
    assert_eq!(objects(&a, "a", "knows")?, vec![term("<b>")?]);
    assert_eq!(predicate_list(&a, "b", true)?, vec![uid(&a, "knows")?]);
    let refused = apply(&a, vec![insert("<a> <knows> <c>")?])
        .err()
        .context("the error of apply(..)")?;
    assert!(
        matches!(&refused.0, ChangesetErrorEnum::Quad(q) if matches!(q.0, QuadErrorEnum::NoSequence { prefix: 1 })),
        "{refused:?}"
    );
    assert_eq!(
        refused.to_string(),
        "No sequence of Resource is held, and the block source leased no block"
    );
    assert_eq!(objects(&a, "a", "knows")?, vec![term("<b>")?]);
    Ok(())
}

#[test]
fn test_a_write_in_another_namespace_keeps_its_keys_and_catalog_there() -> TestResult {
    let a = changesets()?;
    let three = |change| ultragraphdb_core_mapping::Changeset::new(3).with(change);
    let _applied: Applied = a.apply(&three(Change::Declare(
        ultragraphdb_core_mapping::Declaration::Predicate(crate::utils::changesets::declaration(
            "name",
            "with `term` and an exact count index",
        )?),
    )))?;
    let _written: Applied = a.apply(&three(insert(r#"<a> <name> "big apple""#)?))?;
    let namespace_of =
        |k: &Vec<u8>| -> TestResult<u64> { Ok(u64::from_be_bytes(k[1..9].try_into()?)) };
    let keys = stored_keys(&a);
    assert!(!keys.is_empty());
    assert!(
        keys.iter()
            .map(namespace_of)
            .collect::<TestResult<Vec<u64>>>()?
            .iter()
            .all(|n| *n == 3),
        "every key is namespace 3's"
    );
    for keyspace in [Keyspace::ValueIndex, Keyspace::CountIndex] {
        assert!(
            keys.iter()
                .any(|k| matches!(Keyspace::from_byte(k[0]), Some((ks, _)) if ks == keyspace)),
            "{keyspace:?}"
        );
    }
    let name = a
        .quads()
        .id_of(3, ex("name").as_str())?
        .context("<name> has a UID in namespace 3")?;
    assert_eq!(
        a.catalog(3)?.predicate(name),
        Some(&crate::utils::changesets::declaration(
            "name",
            "with `term` and an exact count index"
        )?)
    );
    assert!(a.catalog(NS)?.predicates().next().is_none());
    Ok(())
}

#[test]
fn test_a_delete_naming_a_literal_that_cannot_be_encoded_is_refused_naming_its_predicate()
-> TestResult {
    let a = changesets()?;
    let refused = apply(
        &a,
        vec![delete(
            r#"<a> <embedding> "[0.50,-1.0]"^^ugdb:float32vector"#,
        )?],
    )
    .err()
    .context("the error of apply(..)")?;
    assert!(
        matches!(&refused.0, ChangesetErrorEnum::Literal { predicate, .. } if predicate == ex("embedding").as_str()),
        "{refused:?}"
    );
    Ok(())
}

/// Two subjects given one unique value in one changeset: the second is refused, for the first holds it in the write,
/// and nothing of the changeset is held. The catalog specification's unique values, for a literal, an IRI new to the
/// store and an IRI it holds.
#[test]
fn test_a_unique_value_given_to_two_subjects_in_one_changeset_is_refused() -> TestResult {
    for (object, shown) in [
        (r#""x""#, r#""x""#),
        ("<new>", "<http://ex.org/new>"),
        ("<held>", "<http://ex.org/held>"),
    ] {
        let a = changesets()?;
        declare(&a, "id", "unique")?;
        write(&a, &["<z> <seen> <held>"])?;
        let refused = apply(
            &a,
            vec![
                insert(&format!("<a> <id> {object}"))?,
                insert(&format!("<b> <id> {object}"))?,
            ],
        )
        .err()
        .context("the error of apply(..)")?;
        assert_eq!(
            refused.0,
            ChangesetErrorEnum::Unique {
                predicate: ex("id").as_str().to_string(),
                value: shown.to_string(),
                holder: "<http://ex.org/a>".to_string(),
            },
            "{object}"
        );
        assert!(
            crate::utils::changesets::holds_nothing(&a, "a")?,
            "<a> holds nothing"
        );
        assert!(
            crate::utils::changesets::holds_nothing(&a, "b")?,
            "<b> holds nothing"
        );
        // One subject writing the value twice in a changeset is no conflict.
        write_changes(
            &a,
            vec![
                insert(&format!("<a> <id> {object}"))?,
                insert(&format!("<a> <id> {object}"))?,
            ],
        )?;
        assert_eq!(objects(&a, "a", "id")?, vec![term(object)?], "{object}");
    }
    Ok(())
}

/// A literal object keeps its reverse entry, as every object kind does (index maintenance, reverse edges): the reverse
/// list of the predicate for the literal lists the subject until the statement is deleted.
#[test]
fn test_a_literal_object_keeps_its_reverse_entry() -> TestResult {
    let a = changesets()?;
    write_changes(
        &a,
        vec![
            insert(r#"<a> <name> "Ann""#)?,
            insert(r#"<b> <name> "Ann""#)?,
        ],
    )?;
    let literal = postings(&a, "a", "name")?[0].0.clone();
    let key = QuadKey::Reverse {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        predicate: uid(&a, "name")?,
        object: literal,
    }
    .to_bytes();
    let subjects = |store: &crate::utils::changesets::TestChangesets| -> TestResult<Vec<ultragraphdb_core_ids::SystemId>> {
        Ok(store
            .quads()
            .objects(&key, store.quads().lists().applied_ts())?
            .iter()
            .filter_map(|(o, _)| o.as_id())
            .collect())
    };
    let mut both = vec![uid(&a, "a")?, uid(&a, "b")?];
    both.sort();
    assert_eq!(subjects(&a)?, both);
    write_changes(&a, vec![delete(r#"<a> <name> "Ann""#)?])?;
    assert_eq!(subjects(&a)?, vec![uid(&a, "b")?]);
    write_changes(&a, vec![delete(r#"<b> <name> "Ann""#)?])?;
    assert_eq!(subjects(&a)?, Vec::<ultragraphdb_core_ids::SystemId>::new());
    Ok(())
}

/// A delete in a graph a blank node names finds the graph its own changeset's insert named by that blank node, and
/// none a changeset before it named: a blank node's label names nothing beyond its changeset.
#[test]
fn test_a_delete_in_a_blank_node_graph_finds_only_the_graph_its_changeset_named() -> TestResult {
    let a = changesets()?;
    let in_blank = |delete: bool| -> TestResult<Change> {
        let (graph, subject, predicate, object) = (
            GraphName::from(oxrdf::BlankNode::new_unchecked("g")),
            NamedOrBlankNode::from(ex("a")),
            ex("knows"),
            term("<b>")?,
        );
        Ok(if delete {
            Change::Delete {
                graph,
                subject,
                predicate,
                object,
            }
        } else {
            Change::Insert {
                graph,
                subject,
                predicate,
                object,
            }
        })
    };
    // Inserted and deleted in one changeset: the delete finds the graph, and nothing is listed.
    let both = a.execute(&crate::utils::changesets::changeset(vec![
        in_blank(false)?,
        in_blank(true)?,
    ]))?;
    assert!(
        both.writes().deltas().is_empty(),
        "{:?}",
        both.writes().deltas()
    );
    a.release(&both);
    // Inserted by one changeset, the delete of the next names another graph and removes nothing.
    write_changes(&a, vec![in_blank(false)?])?;
    let later = a.execute(&crate::utils::changesets::changeset(vec![in_blank(true)?]))?;
    assert!(
        later.writes().deltas().is_empty(),
        "{:?}",
        later.writes().deltas()
    );
    Ok(())
}
