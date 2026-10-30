/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Commit-time operations (7.5) run on top of a transaction's execution, against the store at the watermark with an
//! overlay of decided but unapplied writes above it.

use oxrdf::{GraphName, Literal, NamedNode, Term};
use ultragraphdb_core_keys::{DEFAULT_GRAPH, QuadKey};
use ultragraphdb_core_mapping::Change;
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_apply::{
    ChangesetError, ChangesetErrorEnum, Executed, Operation, OperationLink,
};

use crate::utils::changesets::{
    NS, TestChangesets, changeset, changesets, declare, ex, insert, objects, term, uid, write,
};

fn executed(a: &TestChangesets, changes: Vec<Change>) -> Executed {
    a.execute(&changeset(changes)).expect("it executes")
}

/// Runs `operations` on `executed` over an overlay holding `unapplied`'s writes: the transaction as it commits and the
/// results.
fn operate(
    a: &TestChangesets,
    executed: &Executed,
    unapplied: &[&Executed],
    operations: &[Operation],
) -> Result<(Executed, Vec<(u64, bool)>), ChangesetError> {
    let mut overlay = a.quads().lists().batch();
    for e in unapplied {
        overlay.stack(e.writes());
    }
    let dictionary: Vec<_> = unapplied
        .iter()
        .flat_map(|e| e.writes().versionless())
        .collect();
    let mut results = Vec::new();
    let operated = a.operate(executed, operations, &overlay, &dictionary, &mut results)?;
    Ok((operated, results))
}

/// Writes `round` at the next versions, as the commit step's apply does.
fn apply(a: &TestChangesets, round: Vec<Executed>) {
    let mut version = a.quads().lists().applied_ts();
    let round = round
        .into_iter()
        .map(|e| {
            version += 2;
            (version, e)
        })
        .collect();
    a.apply_round(round).expect("the round applies");
}

fn upsert(value: &str, properties: &[(&str, &str)], links: Vec<OperationLink>) -> Operation {
    Operation::UpsertAndLink {
        graph: GraphName::DefaultGraph,
        predicate: ex("id"),
        value: Literal::new_simple_literal(value).into(),
        properties: properties.iter().map(|(p, v)| (ex(p), term(v))).collect(),
        links,
    }
}

fn insert_if_absent(value: &str) -> Operation {
    Operation::InsertIfAbsent {
        graph: GraphName::DefaultGraph,
        predicate: ex("id"),
        value: Literal::new_simple_literal(value).into(),
        properties: Vec::new(),
    }
}

fn cas(subject: &str, predicate: &str, expected: &str, new: &str) -> Operation {
    Operation::CompareAndSet {
        graph: GraphName::DefaultGraph,
        subject: ex(subject).into(),
        predicate: ex(predicate),
        expected: term(expected),
        new: term(new),
    }
}

fn add(subject: &str, predicate: &str, delta: i64) -> Operation {
    Operation::CounterAdd {
        graph: GraphName::DefaultGraph,
        subject: ex(subject).into(),
        predicate: ex(predicate),
        delta,
    }
}

/// The objects of `uid`'s `<predicate>` in the default graph, encoded.
fn objects_of(a: &TestChangesets, subject: u64, predicate: &str) -> Vec<EncodedTerm> {
    let key = QuadKey::Forward {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        predicate: uid(a, predicate),
        subject,
    };
    a.quads()
        .objects(&key.to_bytes(), a.quads().lists().applied_ts())
        .expect("the list reads")
        .into_iter()
        .map(|(o, _)| o)
        .collect()
}

fn unique(a: &TestChangesets) {
    declare(a, "id", "unique, with `exact`");
}

#[test]
fn test_an_insert_if_absent_creates_the_node_once_and_keeps_the_transactions_writes() {
    let a = changesets();
    unique(&a);
    let txn = executed(&a, vec![insert("<m> <kind> \"memory\"")]);
    let writes_before = txn.writes().clone();
    let (operated, results) = operate(&a, &txn, &[], &[insert_if_absent("x")]).unwrap();
    assert_eq!(
        txn.writes(),
        &writes_before,
        "the transaction is left as it was"
    );
    assert_eq!(results.len(), 1);
    let (node, created) = results[0];
    assert!(created && node != 0);
    apply(&a, vec![operated]);
    let held = objects_of(&a, node, "id");
    assert_eq!(held.len(), 1);
    // The node created holds the value asked for.
    assert_eq!(a.quads().term_of(NS, &held[0]).unwrap(), term("\"x\""));
    assert_eq!(objects(&a, "m", "kind"), vec![term("\"memory\"")]);
}

#[test]
fn test_an_insert_if_absent_finds_the_node_holding_the_value() {
    let a = changesets();
    unique(&a);
    write(&a, &["<n> <id> \"x\""]);
    let txn = executed(&a, vec![insert("<m> <kind> \"memory\"")]);
    let (operated, results) = operate(&a, &txn, &[], &[insert_if_absent("x")]).unwrap();
    assert_eq!(results, vec![(uid(&a, "n"), false)]);
    apply(&a, vec![operated]);
    let stats = a
        .quads()
        .statistics(
            NS,
            DEFAULT_GRAPH,
            uid(&a, "id"),
            a.quads().lists().applied_ts(),
        )
        .unwrap();
    assert_eq!(stats.statements(), 1, "nothing created");
}

#[test]
fn test_an_operation_reads_the_unapplied_writes_of_the_overlay() {
    let a = changesets();
    unique(&a);
    let first = executed(&a, vec![insert("<m1> <kind> \"memory\"")]);
    let (first, created) = operate(&a, &first, &[], &[insert_if_absent("x")]).unwrap();
    let second = executed(&a, vec![insert("<m2> <kind> \"memory\"")]);
    let (second, found) = operate(&a, &second, &[&first], &[insert_if_absent("x")]).unwrap();
    assert_eq!(
        found,
        vec![(created[0].0, false)],
        "the unapplied node is found"
    );
    let third = executed(&a, vec![insert("<m3> <kind> \"memory\"")]);
    let (_, alone) = operate(&a, &third, &[], &[insert_if_absent("x")]).unwrap();
    assert!(alone[0].1, "without the overlay the node is not found");
    apply(&a, vec![first, second]);
}

#[test]
fn test_an_upsert_and_link_writes_its_links_both_ways_and_its_properties_only_when_it_creates() {
    let a = changesets();
    unique(&a);
    let links = || {
        vec![
            OperationLink::to_node(ex("has"), ex("m").into()),
            OperationLink::from_node(ex("about"), ex("t").into()),
        ]
    };
    let txn = executed(
        &a,
        vec![
            insert("<m> <kind> \"memory\""),
            insert("<t> <kind> \"topic\""),
        ],
    );
    let (operated, results) = operate(
        &a,
        &txn,
        &[],
        &[upsert("y", &[("label", "\"first\"")], links())],
    )
    .unwrap();
    let node = results[0].0;
    apply(&a, vec![operated]);
    let again = executed(&a, vec![insert("<m> <kind> \"again\"")]);
    let (operated, results) = operate(
        &a,
        &again,
        &[],
        &[upsert("y", &[("label", "\"second\"")], links())],
    )
    .unwrap();
    assert_eq!(results, vec![(node, false)]);
    apply(&a, vec![operated]);
    assert_eq!(
        objects_of(&a, uid(&a, "m"), "has"),
        vec![EncodedTerm::uid(node)]
    );
    assert_eq!(
        objects_of(&a, node, "about"),
        vec![EncodedTerm::uid(uid(&a, "t"))]
    );
    assert_eq!(
        objects_of(&a, node, "label").len(),
        1,
        "the label of the creation only"
    );
    assert_eq!(
        a.quads()
            .list_terms(
                NS,
                &QuadKey::Forward {
                    namespace: NS,
                    graph: DEFAULT_GRAPH,
                    predicate: uid(&a, "label"),
                    subject: node,
                }
                .to_bytes(),
                a.quads().lists().applied_ts()
            )
            .unwrap(),
        vec![term("\"first\"")]
    );
}

#[test]
fn test_an_insert_if_absent_on_a_predicate_not_unique_is_refused() {
    let a = changesets();
    let txn = executed(&a, vec![insert("<m> <kind> \"memory\"")]);
    let refused = operate(&a, &txn, &[], &[insert_if_absent("x")]).unwrap_err();
    assert_eq!(refused, ChangesetError::NotUnique("http://ex.org/id"));
}

/// A string longer than any inline one, so its literal is stored by hash with its dictionary entry.
fn long_text() -> String {
    "x".repeat(ultragraphdb_core_terms::INLINE_MAX + 40)
}

/// An operation reads the dictionary entries the unapplied writes made, as it reads their lists: a long literal's
/// string and an IRI they created are found, not refused or read as corruption (decided 2026-10-28).
#[test]
fn test_an_operation_reads_the_dictionary_entries_of_unapplied_writes() {
    let a = changesets();
    write(&a, &["<a> <seen> true"]);
    let long = format!("\"{}\"", long_text());
    let unapplied = executed(
        &a,
        vec![
            insert(&format!("<a> <note> {long}")),
            insert("<a> <next> <brandnew>"),
        ],
    );
    let txn = executed(&a, vec![insert("<a> <seen> false")]);
    let (_, results) = operate(
        &a,
        &txn,
        &[&unapplied],
        &[cas("a", "note", &long, "\"short\"")],
    )
    .expect("the long string the unapplied write stored is read");
    assert_eq!(results, vec![(0, false)]);
    let (_, results) = operate(
        &a,
        &txn,
        &[&unapplied],
        &[cas("a", "next", "<brandnew>", "<other>")],
    )
    .expect("the IRI the unapplied write created is read");
    assert_eq!(results, vec![(0, false)]);
}

#[test]
fn test_a_compare_and_set_replaces_the_value_it_expects_and_refuses_another() {
    let a = changesets();
    write(&a, &["<a> <status> \"open\""]);
    let txn = executed(&a, vec![insert("<a> <seen> true")]);
    let (operated, results) = operate(
        &a,
        &txn,
        &[],
        &[cas("a", "status", "\"open\"", "\"closed\"")],
    )
    .unwrap();
    assert_eq!(results, vec![(0, false)]);
    apply(&a, vec![operated]);
    assert_eq!(objects(&a, "a", "status"), vec![term("\"closed\"")]);
    let refused = operate(
        &a,
        &txn,
        &[],
        &[cas("a", "status", "\"open\"", "\"again\"")],
    )
    .unwrap_err();
    assert_eq!(
        refused,
        ChangesetError::OperationFailed(Some("\"closed\"".to_string()))
    );
    let none = operate(&a, &txn, &[], &[cas("a", "missing", "\"open\"", "\"x\"")]).unwrap_err();
    assert_eq!(none.0, ChangesetErrorEnum::OperationFailed { found: None });
}

#[test]
fn test_a_counter_add_adds_to_the_value_held_or_to_zero() {
    let a = changesets();
    write(&a, &["<c> <n> 5"]);
    let txn = executed(&a, vec![insert("<other> <kind> \"k\"")]);
    let (operated, results) =
        operate(&a, &txn, &[], &[add("c", "n", 3), add("c", "fresh", -2)]).unwrap();
    // Each result is the counter's new value, an i64 in two's complement (point 33).
    assert_eq!(results, vec![(8, false), ((-2i64) as u64, false)]);
    apply(&a, vec![operated]);
    // 5 + 3, and 0 − 2 for a counter holding nothing.
    assert_eq!(objects(&a, "c", "n"), vec![term("8")]);
    assert_eq!(objects(&a, "c", "fresh"), vec![term("-2")]);
}

#[test]
fn test_a_counter_add_refuses_a_value_that_is_no_integer_or_would_overflow() {
    let a = changesets();
    write(
        &a,
        &["<c> <word> \"five\"", &format!("<c> <max> {}", i64::MAX)],
    );
    let txn = executed(&a, vec![insert("<other> <kind> \"k\"")]);
    let word = operate(&a, &txn, &[], &[add("c", "word", 1)]).unwrap_err();
    assert_eq!(
        word,
        ChangesetError::OperationFailed(Some("\"five\"".to_string()))
    );
    let max = operate(&a, &txn, &[], &[add("c", "max", 1)]).unwrap_err();
    // i64::MAX + 1 overflows; `found` is the value held, in N-Triples.
    assert_eq!(
        max.0,
        ChangesetErrorEnum::OperationFailed {
            found: Some(
                "\"9223372036854775807\"^^<http://www.w3.org/2001/XMLSchema#integer>".to_string()
            )
        }
    );
}

/// A counter add reaching a bound of i64 is applied, and one past it is refused naming the value held: i64::MAX - 1
/// plus 1 is i64::MAX, i64::MIN plus -1 overflows, and i64::MIN plus 0 is i64::MIN.
#[test]
fn test_a_counter_add_reaches_the_bounds_of_i64_and_refuses_to_pass_them() {
    let a = changesets();
    write(
        &a,
        &[
            &format!("<c> <near> {}", i64::MAX - 1),
            &format!("<c> <min> {}", i64::MIN),
        ],
    );
    let txn = executed(&a, vec![insert("<other> <kind> \"k\"")]);
    let below = operate(&a, &txn, &[], &[add("c", "min", -1)]).unwrap_err();
    assert_eq!(
        below.0,
        ChangesetErrorEnum::OperationFailed {
            found: Some(
                "\"-9223372036854775808\"^^<http://www.w3.org/2001/XMLSchema#integer>".to_string()
            )
        }
    );
    let (operated, results) =
        operate(&a, &txn, &[], &[add("c", "near", 1), add("c", "min", 0)]).unwrap();
    assert_eq!(
        results,
        vec![(i64::MAX as u64, false), (i64::MIN as u64, false)]
    );
    apply(&a, vec![operated]);
    assert_eq!(
        objects(&a, "c", "near"),
        vec![term("9223372036854775807")]
    );
    assert_eq!(
        objects(&a, "c", "min"),
        vec![term("-9223372036854775808")]
    );
}

/// A compare-and-set on a value held among others is refused, for the subject's value is not the expected one and
/// nothing else, naming a value held; nothing is replaced.
#[test]
fn test_a_compare_and_set_on_a_subject_holding_several_values_is_refused() {
    let a = changesets();
    write(&a, &["<a> <tag> \"x\"", "<a> <tag> \"y\""]);
    let txn = executed(&a, vec![insert("<a> <seen> true")]);
    let refused = operate(&a, &txn, &[], &[cas("a", "tag", "\"x\"", "\"z\"")]).unwrap_err();
    assert!(
        matches!(
            &refused.0,
            ChangesetErrorEnum::OperationFailed { found: Some(f) } if f == "\"x\"" || f == "\"y\""
        ),
        "{refused:?}"
    );
    let mut held: Vec<String> = objects(&a, "a", "tag").iter().map(Term::to_string).collect();
    held.sort();
    assert_eq!(held, vec!["\"x\"".to_string(), "\"y\"".to_string()]);
}

#[test]
fn test_operations_seed_the_transactions_own_terms() {
    let a = changesets();
    unique(&a);
    // `<m>` is new: the link names the UID the transaction gave it, not a second one.
    let txn = executed(&a, vec![insert("<m> <kind> \"memory\"")]);
    let links = vec![OperationLink::to_node(ex("has"), ex("m").into())];
    let (operated, results) = operate(&a, &txn, &[], &[upsert("w", &[], links)]).unwrap();
    apply(&a, vec![operated]);
    assert_eq!(
        objects_of(&a, uid(&a, "m"), "has"),
        vec![EncodedTerm::uid(results[0].0)]
    );
    assert_eq!(objects(&a, "m", "kind"), vec![term("\"memory\"")]);
    let _: Option<NamedNode> = None;
    let _: Option<Term> = None;
}
