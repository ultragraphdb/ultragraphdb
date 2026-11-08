/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The scenarios of where a transaction's changes and operations go, and of a claim asked of its predicate's home.

use cucumber::{then, when};
use ultragraphdb_common_frontend::{DoubleFrame, last_frame, partition_answer};
use ultragraphdb_common_role::Answer;
use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_protocol_wire::{
    BodyView, Command, CommitOperation, ConditionalWrite, Direction, ErrorCode, Operation,
    PatternSlot, QuadPattern, Read, ReferenceView, RequestTerm, StatementTarget, TermTable, Write,
    WriteChange,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::Steps;
use crate::endpoint_stage::{
    BIND_VERSION, CLUSTER, Client, OBJECT, Options, PREDICATE, Stage, until, view_frame,
};
use crate::endpoint_steps_requests::{answer_claims, run};

const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
/// A datatype the store holds no inline encoding of, which it names by its ID.
const OWN_DATATYPE: &str = "http://ex.org/datatype";
const VALUE_PREDICATE: &str = "http://ex.org/epi.id";
const COUNTER: &str = "http://ex.org/count";

/// The system ID a played home answers a claim's predicate with.
fn predicate_id() -> TestResult<SystemId> {
    Ok(SystemId::new(
        TypePrefix::RESOURCE,
        Bucket::new(1)?,
        Sequence::new(7)?,
    ))
}

/// An insert-if-absent of `value` of the claim predicate `predicate`, its terms.
fn claiming(predicate: &str, value: RequestTerm) -> TestResult<(TermTable, CommitOperation)> {
    let mut terms = TermTable::new();
    let operation = CommitOperation::InsertIfAbsent {
        graph: None,
        predicate: terms.iri(predicate),
        value: terms.intern(value)?,
        properties: Vec::new(),
    };
    Ok((terms, operation))
}

/// A counter's addition at `subject`.
fn counting(subject: RequestTerm) -> TestResult<Operation> {
    let mut terms = TermTable::new();
    let operation = CommitOperation::CounterAdd {
        graph: None,
        subject: terms.intern(subject)?,
        predicate: terms.iri(COUNTER),
        delta: 1,
    };
    Ok(Operation::new(terms, operation))
}

fn literal(lexical: &str, datatype: Option<&str>) -> RequestTerm {
    RequestTerm::Literal {
        lexical: lexical.to_string(),
        datatype: datatype.map(str::to_string),
    }
}

fn language(direction: Option<Direction>) -> RequestTerm {
    RequestTerm::LanguageLiteral {
        lexical: "chat".to_string(),
        language: "fr".to_string(),
        direction,
    }
}

/// A transaction of tenant 0 holding `operation`, committed: its stream.
async fn operating(client: &mut Client, operation: &Operation) -> TestResult<u64> {
    let transaction = client.begin(0).await?;
    client.on(transaction, operation).await?;
    client.commit(transaction).await?;
    Ok(transaction)
}

/// A one-frame insert-if-absent of `value`: its stream.
async fn one_frame_claim(
    client: &mut Client,
    predicate: &str,
    value: RequestTerm,
) -> TestResult<u64> {
    let (terms, operation) = claiming(predicate, value)?;
    client
        .send(&ConditionalWrite::new(
            0,
            terms,
            Vec::new(),
            vec![operation],
        ))
        .await
}

/// A transaction's Operation upserting `value` of `predicate`, committed: its stream.
async fn transaction_claim(
    client: &mut Client,
    predicate: &str,
    value: RequestTerm,
) -> TestResult<u64> {
    let mut terms = TermTable::new();
    let operation = CommitOperation::UpsertAndLink {
        graph: None,
        predicate: terms.iri(predicate),
        value: terms.intern(value)?,
        properties: Vec::new(),
        links: Vec::new(),
    };
    operating(client, &Operation::new(terms, operation)).await
}

#[when(
    "a transaction's Write holds an edge, a delete, a statement's delete by its reifier, a pattern's delete, and a property set on a reifier and deleted from a triple, each naming a subject of partition 1"
)]
fn every_kind_of_change(w: &mut Steps) -> TestResult {
    run(w, Options::default(), |stage| async move {
        let partition = stage.partition(1).await?;
        let subject = stage.on(1)?;
        let mut terms = TermTable::new();
        let (s, p, o) = (terms.iri(&subject), terms.iri(PREDICATE), terms.iri(OBJECT));
        let value = terms.intern(literal("1", None))?;
        let changes = vec![
            WriteChange::InsertEdge {
                graph: None,
                subject: s,
                predicate: p,
                object: o,
                properties: Vec::new(),
            },
            WriteChange::Delete {
                graph: None,
                subject: s,
                predicate: p,
                object: o,
            },
            WriteChange::DeleteStatement {
                graph: None,
                reifier: s,
            },
            WriteChange::DeletePattern {
                graph: None,
                subject: s,
                predicate: p,
            },
            WriteChange::SetProperty {
                statement: StatementTarget::Reifier {
                    graph: None,
                    reifier: s,
                },
                predicate: p,
                value,
            },
            WriteChange::DeleteProperty {
                statement: StatementTarget::Triple {
                    graph: None,
                    subject: s,
                    predicate: p,
                    object: o,
                },
                predicate: p,
                value: None,
            },
        ];
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        client.on(transaction, &Write::new(terms, changes)).await?;
        client.commit(transaction).await?;
        let got = client.got(transaction).await?;
        assert_eq!(got.command, Command::Commit, "{}", got.message);
        assert_eq!(partition.count(Command::Write), 1);
        Ok(())
    })
}

#[then(
    "the Write reaches partition 1 on its transaction's Bind stream, and the Commit is answered"
)]
fn every_kind_of_change_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

/// A read in `transaction` naming nothing of the quad.
fn unbound_read(transaction: u64) -> Read {
    let pattern = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Unbound,
        PatternSlot::Unbound,
        PatternSlot::Unbound,
    );
    Read::new(transaction, 0, TermTable::new(), pattern, Vec::new())
}

/// A compare-and-set of `subject`'s value.
fn comparing(subject: &str) -> Operation {
    let mut terms = TermTable::new();
    let operation = CommitOperation::CompareAndSet {
        graph: None,
        subject: terms.iri(subject),
        predicate: terms.iri(COUNTER),
        expected: terms.iri(OBJECT),
        new: terms.iri(PREDICATE),
    };
    Operation::new(terms, operation)
}

#[when(
    "one transaction adds to a counter of a subject of partition 1, another compares and sets its value, and a third adds to a blank node's counter, and each commits, and a fourth's first read names no subject"
)]
fn operations_go_to_their_subjects(w: &mut Steps) -> TestResult {
    run(w, Options::default(), |stage| async move {
        let partition = stage.partition(1).await?;
        let subject = stage.on(1)?;
        let mut client = stage.ann().await?;
        let operations = [
            counting(RequestTerm::Iri(subject.clone()))?,
            comparing(&subject),
            counting(RequestTerm::BlankNode("b".to_string()))?,
        ];
        let mut transactions = Vec::new();
        for operation in &operations {
            transactions.push(operating(&mut client, operation).await?);
        }
        // A first read naming no subject binds the one partition of the tenant's run.
        let _compute = stage.compute().await?;
        let reading = client.begin(0).await?;
        let read = client.send(&unbound_read(reading)).await?;
        let answered = client.got(read).await?;
        assert_eq!(answered.command, Command::Result, "{}", answered.message);
        for transaction in transactions {
            let got = client.got(transaction).await?;
            assert_eq!(got.command, Command::Commit, "{}", got.message);
        }
        assert_eq!(partition.count(Command::Operation), operations.len());
        Ok(())
    })
}

#[then(
    "each Operation reaches the partition on its transaction's Bind stream, each Commit is answered, and the read binds the tenant's one partition"
)]
fn operations_go_to_their_subjects_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "ConditionalWrites insert-if-absent a typed literal, a language-tagged one, one of each base direction, an IRI and a literal of a datatype the store names by its ID, and a transaction's Operation upserts a typed literal"
)]
fn every_literal_form_is_claimed(w: &mut Steps) -> TestResult {
    run(w, Options::default(), |stage| async move {
        let id = predicate_id()?;
        let partition = stage
            .partition_with(1, move |frame| answer_claims(frame, id))
            .await?;
        let mut client = stage.ann().await?;
        let claimed = [
            literal("5", Some(XSD_INTEGER)),
            language(None),
            language(Some(Direction::Ltr)),
            language(Some(Direction::Rtl)),
        ];
        let in_turn = [
            RequestTerm::Iri(OBJECT.to_string()),
            literal("x", Some(OWN_DATATYPE)),
        ];
        let mut streams = Vec::new();
        for value in claimed.iter().chain(in_turn.iter()) {
            streams.push(one_frame_claim(&mut client, VALUE_PREDICATE, value.clone()).await?);
        }
        streams.push(
            transaction_claim(
                &mut client,
                VALUE_PREDICATE,
                literal("6", Some(XSD_INTEGER)),
            )
            .await?,
        );
        for stream in streams {
            let got = client.got(stream).await?;
            assert_eq!(got.command, Command::Commit, "{}", got.message);
        }
        assert_eq!(
            partition.count(Command::Reference),
            claimed.len() + 1,
            "one claim asked for each literal claimed"
        );
        Ok(())
    })
}

#[then(
    "each literal's claim is asked of its predicate's home before the write leaves, the IRI and the datatype named by ID go in turn, and each is answered"
)]
fn every_literal_form_is_claimed_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

/// A home refusing to register every predicate a claim asks of it, and answering the rest as nothing goes wrong.
fn refusing_home(frame: &DoubleFrame) -> Answer {
    if frame.command() != Command::Reference {
        return partition_answer(frame, CLUSTER, BIND_VERSION);
    }
    let reply = view_frame(frame.bytes())
        .ok()
        .and_then(|view| ReferenceView::from_frame(&view).ok()?.to_body().ok())
        .map(|asked| {
            let mut reply = asked.into_reply();
            let _refused = reply.refuse(0, ErrorCode::Refused.number());
            reply
        });
    reply
        .and_then(|reply| last_frame(CLUSTER, frame.stream(), &reply).ok())
        .map_or(Answer::Done, Answer::Reply)
}

/// A claim of a one-frame write and of a transaction's Operation, each refused with `code`.
async fn both_refused(stage: &Stage, predicate: &str, code: ErrorCode) -> TestResult {
    let mut client = stage.ann().await?;
    let value = || literal("5", Some(XSD_INTEGER));
    let one_frame = one_frame_claim(&mut client, predicate, value()).await?;
    let transaction = transaction_claim(&mut client, predicate, value()).await?;
    for stream in [one_frame, transaction] {
        let (refused, _detail) = client.refused(stream).await?;
        assert_eq!(refused, code);
    }
    client.silent(transaction, 5).await?;
    Ok(())
}

#[when(
    "the home of a claim's predicate refuses to register it, for a ConditionalWrite and for a transaction's Operation"
)]
fn a_refusing_home_refuses_the_claim(w: &mut Steps) -> TestResult {
    run(w, Options::default(), |stage| async move {
        let _home = stage.partition_with(1, refusing_home).await?;
        both_refused(&stage, VALUE_PREDICATE, ErrorCode::Refused).await
    })
}

#[then("each is refused with code 7, and the transaction ends")]
fn a_refusing_home_refuses_the_claim_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a claim's predicate is homed on a partition no process joined, for a ConditionalWrite and for a transaction's Operation"
)]
fn a_claim_with_no_home_is_unavailable(w: &mut Steps) -> TestResult {
    let options = Options {
        partitions: 2,
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _partition = stage.partition(1).await?;
        let predicate = stage.on(2)?;
        both_refused(&stage, &predicate, ErrorCode::Unavailable).await
    })
}

#[then("each is refused as unavailable, and the transaction ends")]
fn a_claim_with_no_home_is_unavailable_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "the connection to the home of a claim's predicate closes while it holds the claims of a ConditionalWrite and of a transaction's Operation"
)]
fn a_home_closing_under_claims(w: &mut Steps) -> TestResult {
    run(w, Options::default(), |stage| async move {
        let home = stage
            .partition_with(1, |frame| {
                if frame.command() == Command::Reference {
                    return Answer::Open;
                }
                partition_answer(frame, CLUSTER, BIND_VERSION)
            })
            .await?;
        let mut client = stage.ann().await?;
        let value = || literal("5", Some(XSD_INTEGER));
        let one_frame = one_frame_claim(&mut client, VALUE_PREDICATE, value()).await?;
        let transaction = transaction_claim(&mut client, VALUE_PREDICATE, value()).await?;
        until("both claims held", || home.count(Command::Reference) == 2).await?;
        home.close();
        for stream in [one_frame, transaction] {
            assert_eq!(client.refused(stream).await?.0, ErrorCode::Unavailable);
        }
        Ok(())
    })
}

#[when(
    "a transaction's first Write names only a blank node while no partition is joined, another's names a subject of a partition no process joined, and a third's first read names no subject of a tenant whose run spans two partitions"
)]
fn first_requests_bind_or_are_unavailable(w: &mut Steps) -> TestResult {
    let options = Options {
        partitions: 2,
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let mut client = stage.ann().await?;
        // No partition is joined yet: a blank node's write finds none in turn.
        let blank = client.begin(0).await?;
        let mut terms = TermTable::new();
        let insert = WriteChange::Insert {
            graph: None,
            subject: terms.intern(RequestTerm::BlankNode("b".to_string()))?,
            predicate: terms.iri(PREDICATE),
            object: terms.iri(OBJECT),
        };
        client.on(blank, &Write::new(terms, vec![insert])).await?;
        assert_eq!(client.refused(blank).await?.0, ErrorCode::Unavailable);
        let run_partition = *stage
            .links
            .run_partitions(0)
            .first()
            .context("the tenant's run")?;
        let unjoined = if run_partition == 1 { 2 } else { 1 };
        let _partition = stage.partition(run_partition).await?;
        let elsewhere = client.begin(0).await?;
        client.write(elsewhere, &stage.on(unjoined)?).await?;
        assert_eq!(client.refused(elsewhere).await?.0, ErrorCode::Unavailable);
        let reading = client.begin(0).await?;
        let read = client.send(&unbound_read(reading)).await?;
        let (code, detail) = client.refused(read).await?;
        assert_eq!(code, ErrorCode::CrossPartition);
        let reached = stage.links.run_partitions(0);
        let named = [detail & u64::from(u32::MAX), detail >> 32];
        assert!(
            named
                .iter()
                .all(|partition| reached.iter().any(|held| u64::from(*held) == *partition)),
            "{detail:#x} names the run {reached:?}"
        );
        Ok(())
    })
}

#[then("the first two are refused as unavailable, and the read as reaching both partitions")]
fn first_requests_bind_or_are_unavailable_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}
