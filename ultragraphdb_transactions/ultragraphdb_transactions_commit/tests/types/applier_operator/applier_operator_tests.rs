/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::GraphName;
use ultragraphdb_store_apply::Operation;
use ultragraphdb_transactions_commit::{ApplierOperator, CommitErrorEnum, Operator};

use ultragraphdb_protocol_wire::WriteSetRecordView;

use crate::utils::fixture::{applier, begin, ex, insert, insert_literal, prepare, setup, write};

fn counter(predicate: &str, delta: i64) -> Operation {
    Operation::CounterAdd {
        graph: GraphName::DefaultGraph,
        subject: ex("c").into(),
        predicate: ex(predicate),
        delta,
    }
}

/// The overlay is built at a round's first operation from the unapplied proposals, takes each admitted one, and is
/// dropped when the next round begins, so it never holds more than the rounds it was built for (7.5).
#[test]
fn test_the_overlay_lives_one_round_and_reads_the_unapplied_proposals() {
    let applier = applier();
    setup(
        &applier,
        vec![
            insert("a", "knows", "b"),
            insert_literal("c", "word", "zero"),
        ],
    );
    let mut operator = ApplierOperator::new(&applier);
    assert!(std::ptr::eq(operator.applier(), &applier));
    operator.begin_round();
    assert!(!operator.has_overlay());
    // A proposal adding 5 to a counter holding nothing.
    let mut five = begin(&applier);
    five.operate(counter("n", 5), 0).unwrap();
    let mut five = prepare(five);
    let mut results = Vec::new();
    let staged = operator
        .operate(&five, std::iter::empty(), &mut results)
        .expect("operated");
    assert!(operator.has_overlay());
    // The counter's new value, 0 + 5, created 0 as a counter add's always is (PROTOCOL.md §6.3).
    assert_eq!(results, vec![ultragraphdb_protocol_wire::OperationResult::new(5, false)]);
    operator.adopt(&mut five, staged);
    // A counter of "zero" is no integer: refused with the value found.
    let mut refused = begin(&applier);
    refused.operate(counter("word", 1), 0).unwrap();
    let refused = prepare(refused);
    let Err(error) = operator.operate(&refused, std::iter::empty(), &mut results) else {
        panic!("a string is no counter");
    };
    // The value found, as an N-Triples term: the plain literal "zero".
    assert_eq!(
        error.0,
        CommitErrorEnum::OperationFailed {
            found: Some("\"zero\"".to_string())
        }
    );
    operator.begin_round();
    assert!(
        !operator.has_overlay(),
        "a round begins without the last one's overlay"
    );
}

/// An operation naming an IRI that a decided but unapplied proposal created takes that proposal's UID: the creator's
/// round writes the dictionary entry, so the operation's record writes none and creates no second UID (7.5).
#[test]
fn test_an_operation_on_an_iri_an_unapplied_proposal_created_reuses_its_uid() {
    let applier = applier();
    setup(&applier, vec![insert("a", "knows", "b")]);
    let mut operator = ApplierOperator::new(&applier);
    operator.begin_round();
    let mut creator = begin(&applier);
    write(&mut creator, vec![insert("fresh", "knows", "b")]);
    let creator = prepare(creator);
    let fresh = ex("fresh");
    assert!(
        creator
            .executed()
            .terms()
            .created()
            .any(|(iri, _)| iri == fresh.as_str())
    );
    let mut adder = begin(&applier);
    adder
        .operate(
            Operation::CounterAdd {
                graph: GraphName::DefaultGraph,
                subject: fresh.clone().into(),
                predicate: ex("n"),
                delta: 1,
            },
            0,
        )
        .unwrap();
    let adder = prepare(adder);
    let mut results = Vec::new();
    let staged = operator
        .operate(&adder, std::iter::once(&creator), &mut results)
        .expect("operated");
    let view = WriteSetRecordView::new(operator.staged_record(&staged)).unwrap();
    let names_fresh = |bytes: &[u8]| {
        bytes
            .windows(fresh.as_str().len())
            .any(|w| w == fresh.as_str().as_bytes())
    };
    assert!(
        view.dictionary()
            .iter()
            .all(|e| !names_fresh(e.key()) && !e.value().is_some_and(names_fresh)),
        "the operation writes no dictionary entry of an IRI the unapplied proposal created"
    );
}
