/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::utils::fixture::Refused;
use oxrdf::GraphName;
use ultragraphdb_store_apply::Operation;
use ultragraphdb_tool_bdd::{Context, TestResult};
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
fn test_the_overlay_lives_one_round_and_reads_the_unapplied_proposals() -> TestResult {
    let applier = applier()?;
    let _version: u64 = setup(
        &applier,
        vec![
            insert("a", "knows", "b"),
            insert_literal("c", "word", "zero"),
        ],
    )?;
    let mut operator = ApplierOperator::new(&applier);
    assert!(std::ptr::eq(operator.applier(), &applier));
    operator.begin_round();
    assert!(!operator.has_overlay());
    // A proposal adding 5 to a counter holding nothing.
    let mut five = begin(&applier)?;
    five.operate(counter("n", 5), 0)?;
    let mut five = prepare(five)?;
    let mut results = Vec::new();
    let staged = operator
        .operate(&five, std::iter::empty(), &mut results)
        .context("operated")?;
    assert!(operator.has_overlay());
    // The counter's new value, 0 + 5, created 0 as a counter add's always is (PROTOCOL.md §6.3).
    assert_eq!(
        results,
        vec![ultragraphdb_protocol_wire::OperationResult::counter(5)]
    );
    operator.adopt(&mut five, staged);
    // A counter of "zero" is no integer: refused with the value found.
    let mut refused = begin(&applier)?;
    refused.operate(counter("word", 1), 0)?;
    let refused = prepare(refused)?;
    let error = operator
        .operate(&refused, std::iter::empty(), &mut results)
        .refused("a string is no counter")?;
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
    Ok(())
}

/// An operation naming an IRI that a decided but unapplied proposal created takes that proposal's ID: the dictionary
/// shares one ID among concurrent creators (term-dictionary, "An IRI's ID is got or created at its home"), so the
/// operation's writes name the IRI by the creator's ID and create no second one (7.5).
#[test]
fn test_an_operation_on_an_iri_an_unapplied_proposal_created_reuses_its_id() -> TestResult {
    let applier = applier()?;
    let _version: u64 = setup(&applier, vec![insert("a", "knows", "b")])?;
    let mut operator = ApplierOperator::new(&applier);
    operator.begin_round();
    let mut creator = begin(&applier)?;
    write(&mut creator, vec![insert("fresh", "knows", "b")])?;
    let creator = prepare(creator)?;
    let fresh = ex("fresh");
    let created = creator
        .executed()
        .terms()
        .written()
        .find(|(iri, _)| *iri == fresh.as_str())
        .map(|(_, id)| id)
        .context("the creator wrote the IRI")?;
    let mut adder = begin(&applier)?;
    adder.operate(
        Operation::CounterAdd {
            graph: GraphName::DefaultGraph,
            subject: fresh.clone().into(),
            predicate: ex("n"),
            delta: 1,
        },
        0,
    )?;
    let adder = prepare(adder)?;
    let mut results = Vec::new();
    let staged = operator
        .operate(&adder, std::iter::once(&creator), &mut results)
        .context("operated")?;
    let view = WriteSetRecordView::new(operator.staged_record(&staged))?;
    let subject_prefix = [vec![0u8; 16], created.to_be_bytes().to_vec()].concat();
    assert!(
        view.inserted()
            .iter()
            .any(|e| e.key().starts_with(&subject_prefix)),
        "the counter's statement names the creator's ID"
    );
    Ok(())
}
