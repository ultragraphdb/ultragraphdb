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

/// What an operator bound to `partition` of two answers `operation` with in a transaction of its own: `Ok` when it
/// operates, the staged result discarded and the proposal released; the refusal otherwise.
fn bound_answer(
    applier: &crate::utils::fixture::Applier,
    partition: u32,
    operation: Operation,
) -> TestResult<Result<(), ultragraphdb_transactions_commit::CommitError>> {
    use ultragraphdb_cluster_placement::{NodeId, PartitionId, PlacementMap};
    let placement = PlacementMap::even(2, NodeId::new(1)).context("the placement")?;
    let partition = PartitionId::new(partition).context("the partition")?;
    let mut operator = ApplierOperator::new(applier).bound(partition, &placement);
    operator.begin_round();
    let mut txn = begin(applier)?;
    txn.operate(operation, 0)?;
    let proposal = prepare(txn)?;
    let mut results = Vec::new();
    let answer = match operator.operate(&proposal, std::iter::empty(), &mut results) {
        Ok(staged) => {
            operator.discard(&proposal, staged);
            Ok(())
        }
        Err(refused) => Err(refused),
    };
    let _buffers = proposal.release(applier);
    Ok(answer)
}

fn insert_if_absent(value: oxrdf::Term) -> Operation {
    Operation::InsertIfAbsent {
        graph: GraphName::DefaultGraph,
        predicate: ex("key"),
        value,
        properties: Vec::new(),
    }
}

fn unique_key(applier: &crate::utils::fixture::Applier) -> TestResult {
    use ultragraphdb_core_mapping::{Change, Declaration, PredicateDeclaration};
    let declared = PredicateDeclaration::new(ex("key")).with_unique(true);
    let _version: u64 = setup(
        applier,
        vec![Change::Declare(Declaration::Predicate(declared))],
    )?;
    Ok(())
}

/// A claim of a literal or of a held IRI is taken by exactly one of two partitions, the one owning its bucket: the
/// other refuses it cross-partition naming itself and the owner (transactions, "A claim owned by another partition is
/// cross-partition"). Both partitions are seen to own some.
#[test]
fn test_a_claim_is_taken_by_the_partition_owning_its_bucket_alone() -> TestResult {
    use ultragraphdb_transactions_commit::CommitError;
    let applier = applier()?;
    unique_key(&applier)?;
    let objects: Vec<_> = (0..16)
        .map(|i| insert("held", "links", &format!("o{i}")))
        .collect();
    let _version: u64 = setup(&applier, objects)?;
    let values = (0..16)
        .map(|i| oxrdf::Term::from(oxrdf::Literal::new_simple_literal(format!("v{i}"))))
        .chain((0..16).map(|i| oxrdf::Term::from(ex(&format!("o{i}")))));
    let mut owners = [0u32; 2];
    for value in values {
        let one = bound_answer(&applier, 1, insert_if_absent(value.clone()))?;
        let two = bound_answer(&applier, 2, insert_if_absent(value.clone()))?;
        match (one, two) {
            (Ok(()), Err(refused)) => {
                assert_eq!(refused, CommitError::cross_partition(2, 1), "{value}");
                owners[0] += 1;
            }
            (Err(refused), Ok(())) => {
                assert_eq!(refused, CommitError::cross_partition(1, 2), "{value}");
                owners[1] += 1;
            }
            other => {
                return Err(ultragraphdb_tool_bdd::TestError::new(format!(
                    "{value} not owned by exactly one partition: {other:?}"
                )));
            }
        }
    }
    assert!(owners.iter().all(|n| *n > 0), "{owners:?}");
    Ok(())
}

/// An operation claiming nothing is taken by either partition: a counter add; an insert-if-absent of an IRI the
/// dictionary does not hold, of a blank node, or of a literal whose datatype the dictionary does not hold, none of which
/// any value the store holds names.
#[test]
fn test_an_operation_claiming_nothing_is_taken_by_either_partition() -> TestResult {
    let applier = applier()?;
    unique_key(&applier)?;
    let unheld_type = oxrdf::NamedNode::new_unchecked("http://ex.org/type/never");
    let operations = [
        counter("n", 1),
        insert_if_absent(ex("never").into()),
        insert_if_absent(oxrdf::BlankNode::new_unchecked("b0").into()),
        insert_if_absent(oxrdf::Literal::new_typed_literal("1", unheld_type).into()),
    ];
    for operation in operations {
        for partition in [1, 2] {
            assert_eq!(
                bound_answer(&applier, partition, operation.clone())?,
                Ok(()),
                "{partition}: {operation:?}"
            );
        }
    }
    Ok(())
}

/// An insert-if-absent on a predicate the dictionary does not hold claims nothing, and the store's refusal of it is
/// the commit step's refused, carrying the store's text.
#[test]
fn test_an_operation_the_store_refuses_is_refused_with_its_text() -> TestResult {
    use ultragraphdb_protocol_wire::ErrorCode;
    let applier = applier()?;
    let operation = Operation::InsertIfAbsent {
        graph: GraphName::DefaultGraph,
        predicate: ex("undeclared"),
        value: oxrdf::Literal::new_simple_literal("x").into(),
        properties: Vec::new(),
    };
    let refused = bound_answer(&applier, 1, operation)?.refused("the store refuses it")?;
    assert_eq!(
        refused.0,
        CommitErrorEnum::Refused {
            reason: "Predicate <http://ex.org/undeclared> is not declared unique, so no node is found by its value"
                .to_string()
        }
    );
    assert_eq!(refused.code(), ErrorCode::Refused);
    assert!(!refused.is_retryable());
    Ok(())
}
