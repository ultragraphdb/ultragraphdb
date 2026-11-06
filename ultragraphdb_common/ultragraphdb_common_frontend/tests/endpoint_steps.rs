/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The client endpoint's scenarios, each run on a stage of its own. A scenario's whole exchange runs in its `When`, which asserts as
//! the exchange goes, since what a later frame means depends on an earlier one; its `Then` holds that it ran.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use cucumber::{then, when};
use ultragraphdb_common_frontend::{
    DoubleFrame, EndpointLimits, EveryAuthenticated, GateScope, TenantGate, batch_frame,
    empty_batch, last_frame, partition_answer,
};
use ultragraphdb_common_role::{Answer, RoleClient};
use ultragraphdb_protocol_transport::BufferPool;
use ultragraphdb_protocol_transport::Framer;
use ultragraphdb_protocol_transport_compio::{Carrier, Connection};
use ultragraphdb_protocol_wire::{
    Action, Begin, BodyBuilder, BodyView, Command, Commit, CommitOperation, CommitView,
    ComputeReadView, ConditionalWrite, Credit, DeclaredPredicate, Describe, DirectWrite,
    DropCommand, ErrorCode, ErrorReply, ErrorReplyView, Header, Hello, Install, Language,
    MAX_FRAME_SIZE, Prepare, Query, ReadLevel, RequestTerm, TermTable, Write, WriteChange,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::Steps;
use crate::endpoint_stage::{
    BIND_VERSION, CLUSTER, OBJECT, Options, PREDICATE, Stage, empty_answer, fresh, insert_into,
    pause, read_of, until, view_frame, write_of,
};
use crate::endpoint_steps_requests::{
    Deny, answer_frame, answer_with, bucket_of, challenge, cross_refusal, declaring,
    held_directory, limited, names, prepared_in, run, stalling,
};

fn wrong_owner(frame: &DoubleFrame, bucket: u32, owner: u32) -> Answer {
    if frame.command() == Command::Write || frame.command() == Command::DirectWrite {
        return Answer::Refuse(
            ErrorReply::new(ErrorCode::WrongPartition, "another partition owns it")
                .with_detail(u64::from(bucket) | (u64::from(owner) << 32)),
        );
    }
    partition_answer(frame, CLUSTER, BIND_VERSION)
}

#[when("the frontend allows 2 connections and a third client connects")]
fn connections_past_the_bound_are_refused(w: &mut Steps) -> TestResult {
    let options = Options {
        connections: 2,
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let mut first = stage.ann().await?;
        let mut second = stage.ann().await?;
        let third = RoleClient::dial(&stage.client, CLUSTER, "ann", "pw").await;
        let refused = third.err().context("a refusal")?;
        assert!(
            refused.to_string().contains("15") || refused.to_string().contains("connections"),
            "{refused}"
        );
        let _first = first.begin(0).await?;
        let _second = second.begin(0).await?;
        Ok(())
    })
}

#[then("the third connection is refused with the limit error, and the first two keep working")]
fn connections_past_the_bound_are_refused_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("one connection's commit waits on its owner check")]
fn a_blocked_commit_does_not_block_other_connections(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let held = stage
            .partition_with(1, |frame| {
                if frame.command() == Command::Commit {
                    return Answer::Open;
                }
                partition_answer(frame, CLUSTER, BIND_VERSION)
            })
            .await?;
        let subject = stage.on(1)?;
        let mut waiting = stage.ann().await?;
        let transaction = waiting.begin(0).await?;
        waiting.write(transaction, &subject).await?;
        waiting.commit(transaction).await?;
        until("the Commit", || held.count(Command::Commit) == 1).await?;
        let mut other = stage.ann().await?;
        let second = other.begin(0).await?;
        other.write(second, &subject).await?;
        until("the other's Write", || held.count(Command::Write) == 2).await?;
        waiting.silent(transaction, 20).await?;
        Ok(())
    })
}

#[then("reads and writes of other connections on that core go on")]
fn a_blocked_commit_does_not_block_other_connections_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a client authenticates as `ann` with a wrong password, and as an unknown name")]
fn a_wrong_password_is_refused_without_detail(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let wrong = RoleClient::dial(&stage.client, CLUSTER, "ann", "not it").await;
        let unknown = RoleClient::dial(&stage.client, CLUSTER, "nobody", "pw").await;
        let (wrong, unknown) = (
            wrong.err().context("a refusal")?,
            unknown.err().context("a refusal")?,
        );
        assert_eq!(wrong.to_string(), unknown.to_string());
        Ok(())
    })
}

#[then("both are refused with the same authentication error")]
fn a_wrong_password_is_refused_without_detail_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a client sends a Read before authenticating")]
fn a_data_request_before_authentication_is_refused(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let carrier = Carrier::connect(&stage.client).await?;
        let mut raw = Connection::new(carrier, Framer::new(CLUSTER, MAX_FRAME_SIZE));
        let pool = BufferPool::new(1, MAX_FRAME_SIZE);
        let _welcome = raw
            .hello(&pool, &Hello::new(1, 1, 1, MAX_FRAME_SIZE as u32, "client"))
            .await?;
        let mut frame =
            read_of(1, OBJECT).frame(Header::new(CLUSTER, Command::Read).with_stream(1))?;
        Header::stamp_sequence(&mut frame, 2)?;
        let _sent = raw.write_frame(frame).await?;
        let answer = raw.read_frame(&pool).await?;
        let view = answer.view()?;
        assert_eq!(view.header().stream(), 1, "on the request's stream");
        assert_eq!(
            ErrorReplyView::from_frame(&view)?.code(),
            ErrorCode::Unauthenticated
        );
        drop(answer);
        let mut again =
            Begin::new(0, 0, false).frame(Header::new(CLUSTER, Command::Begin).with_stream(2))?;
        Header::stamp_sequence(&mut again, 3)?;
        let _sent_2 = raw.write_frame(again).await?;
        let answer_2 = raw.read_frame(&pool).await?;
        assert_eq!(
            ErrorReplyView::from_frame(&answer_2.view()?)?.code(),
            ErrorCode::Unauthenticated,
            "it stays open"
        );
        Ok(())
    })
}

#[then(
    "the request is refused with the authentication error, and the connection stays open for authentication"
)]
fn a_data_request_before_authentication_is_refused_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a client asks to authenticate as an unknown name, twice, across a restart of the frontend")]
fn an_unknown_name_answers_as_a_known_one_does(w: &mut Steps) -> TestResult {
    let options = Options {
        directory: Some(fresh()?),
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let directory = held_directory(&stage)?;
        let first = challenge(&stage.client, "nobody").await?;
        let wrong = RoleClient::dial(&stage.client, CLUSTER, "ann", "not it")
            .await
            .err()
            .context("a refusal")?;
        let unknown = RoleClient::dial(&stage.client, CLUSTER, "nobody", "pw")
            .await
            .err()
            .context("a refusal")?;
        assert_eq!(wrong.to_string(), unknown.to_string());
        stage.stop().await?;
        let stage_2 = Stage::start(Options {
            directory: Some(directory),
            ..Options::default()
        })
        .await?;
        assert_eq!(first, challenge(&stage_2.client, "nobody").await?);
        Ok(())
    })
}

#[then(
    "both challenges carry the same salt and iteration count, and the exchange fails with the same authentication error as a wrong password"
)]
fn an_unknown_name_answers_as_a_known_one_does_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a client authenticates as the unknown name `nobody`, the frontend restarts, and a client authenticates as `nobody` on another core"
)]
fn an_unknown_name_keeps_one_salt_across_a_restart(w: &mut Steps) -> TestResult {
    let options = Options {
        directory: Some(fresh()?),
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let directory = held_directory(&stage)?;
        let first = challenge(&stage.client, "nobody").await?;
        stage.stop().await?;
        let stage_2 = Stage::start(Options {
            directory: Some(directory),
            ..Options::default()
        })
        .await?;
        assert_eq!(
            first,
            challenge(&stage_2.client, "nobody").await?,
            "one salt, from the log"
        );
        Ok(())
    })
}

#[then(
    "both challenges carry one salt, and the metadata group's log holds the secret it derives from"
)]
fn an_unknown_name_keeps_one_salt_across_a_restart_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a client writes one statement per partition, each subject an IRI of that partition's buckets"
)]
fn a_write_reaches_its_partition(w: &mut Steps) -> TestResult {
    let options = Options {
        partitions: 2,
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let partitions = [stage.partition(1).await?, stage.partition(2).await?];
        let mut client = stage.ann().await?;
        for (at, partition) in (1..).zip(&partitions) {
            let subject = stage.on(at)?;
            let transaction = client.begin(0).await?;
            client.write(transaction, &subject).await?;
            client.commit(transaction).await?;
            assert_eq!(client.got(transaction).await?.command, Command::Commit);
            assert_eq!(
                partition.commands(&[]),
                [Command::Bind, Command::Write, Command::Commit]
            );
        }
        Ok(())
    })
}

#[then(
    "the frontend sends each Write first to the partition owning its subject's bucket, and each commit's reply reaches the client"
)]
fn a_write_reaches_its_partition_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a partition refuses a write naming its new owner")]
fn a_stale_placement_map_is_corrected(w: &mut Steps) -> TestResult {
    let options = Options {
        partitions: 2,
        older_map: true,
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let subject = stage.on(2)?;
        let bucket = bucket_of(&subject)?;
        let old = stage
            .partition_with(1, move |frame| wrong_owner(frame, bucket, 2))
            .await?;
        let owner = stage.partition(2).await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        client.write(transaction, &subject).await?;
        client.commit(transaction).await?;
        assert_eq!(
            client.got(transaction).await?.command,
            Command::Commit,
            "the owner's reply"
        );
        assert_eq!(old.count(Command::Write), 1, "resent once");
        assert_eq!(
            owner.commands(&[]),
            [Command::Bind, Command::Write, Command::Commit]
        );
        Ok(())
    })
}

#[then(
    "the frontend refreshes its map, resends the write to the owner once, and the client receives the owner's reply"
)]
fn a_stale_placement_map_is_corrected_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a partition answers a client's first Write with code 26 of `detail` 0, a home having refused its registration"
)]
fn a_homes_code_26_reaches_the_client(w: &mut Steps) -> TestResult {
    let options = Options {
        partitions: 2,
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let subject = stage.on(1)?;
        let bound = stage
            .partition_with(1, |frame| wrong_owner(frame, 0, 0))
            .await?;
        let other = stage.partition(2).await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        client.write(transaction, &subject).await?;
        assert_eq!(
            client.refused(transaction).await?,
            (ErrorCode::WrongPartition, 0)
        );
        assert_eq!(bound.count(Command::Write), 1, "sent once");
        assert_eq!(
            other.commands(&[]),
            [] as [Command; 0],
            "no other partition"
        );
        Ok(())
    })
}

#[then(
    "the client receives code 26 on the transaction's stream, and the frontend binds no other partition"
)]
fn a_homes_code_26_reaches_the_client_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a client sends two Writes of a new transaction without waiting, and the partition refuses the first with code 26"
)]
fn later_requests_wait_for_the_first(w: &mut Steps) -> TestResult {
    let options = Options {
        partitions: 2,
        older_map: true,
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let (first, second) = (stage.on_tagged(2, "a")?, stage.on_tagged(2, "b")?);
        let bucket = bucket_of(&first)?;
        let old = stage
            .partition_with(1, move |frame| wrong_owner(frame, bucket, 2))
            .await?;
        let owner = stage.partition(2).await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        client.write(transaction, &first).await?;
        client.write(transaction, &second).await?;
        until("both at the owner", || owner.count(Command::Write) == 2).await?;
        assert_eq!(
            old.count(Command::Write),
            1,
            "the second went to no partition before the answer"
        );
        let writes = owner.frames(Command::Write);
        assert!(names(writes.first(), &first)?, "first");
        assert!(names(writes.get(1), &second)?, "then second");
        client.silent(transaction, 20).await?;
        Ok(())
    })
}

#[then(
    "the second was sent to no partition; the frontend refreshes its map, binds to the owner, and sends the first and then the second, and the client sees no error"
)]
fn later_requests_wait_for_the_first_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a transaction's first request is a read, compute answers code 23, and the refreshed map moves the read's subject to partition 2"
)]
fn a_first_read_moved_by_a_stale_map_is_resent(w: &mut Steps) -> TestResult {
    let options = Options {
        partitions: 2,
        older_map: true,
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let subject = stage.on(2)?;
        let old = stage.partition(1).await?;
        let owner = stage.partition(2).await?;
        let answered = Rc::new(Cell::new(0));
        let count = Rc::clone(&answered);
        let _compute = stage
            .compute_with(move |frame| {
                count.set(count.get() + 1);
                if count.get() == 1 {
                    return Answer::Refuse(cross_refusal(1, 2));
                }
                empty_answer(frame, 2)
            })
            .await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        let read = client.read(transaction, &subject).await?;
        assert_eq!(client.got(read).await?.command, Command::Result, "its rows");
        assert_eq!(old.count(Command::Cancel), 1, "the Bind stream cancelled");
        assert_eq!(owner.count(Command::Bind), 1, "bound to the owner");
        assert_eq!(answered.get(), 2, "resent once");
        Ok(())
    })
}

#[then(
    "the frontend cancels the Bind stream, binds to partition 2 and resends the read once, and the client receives its rows"
)]
fn a_first_read_moved_by_a_stale_map_is_resent_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a transaction's first request is a read, compute answers code 23, and the refreshed map leaves the read's subject where it was"
)]
fn a_first_read_the_map_does_not_move_is_refused(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let subject = stage.on(1)?;
        let _partition = stage.partition(1).await?;
        let _compute = stage
            .compute_with(|_| Answer::Refuse(cross_refusal(1, 2)))
            .await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        let read = client.read(transaction, &subject).await?;
        assert_eq!(client.refused(read).await?.0, ErrorCode::CrossPartition);
        client.write(transaction, &subject).await?;
        assert_eq!(
            client.refused(transaction).await?.0,
            ErrorCode::CrossPartition
        );
        Ok(())
    })
}

#[then(
    "the client's read is refused with code 23, and its next request on the transaction's stream too"
)]
fn a_first_read_the_map_does_not_move_is_refused_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "reads A and C of a read-write transaction are open and a Write is held behind them, compute answers A with code 23, and the client then sends read D"
)]
fn a_read_s_code_23_answers_the_transaction_s_other_reads(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let subject = stage.on(1)?;
        let partition = stage.partition(1).await?;
        let compute = stage.compute_with(|_| Answer::Open).await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        let first = client.read(transaction, &subject).await?;
        until("the first read", || {
            compute.count(Command::ComputeRead) == 1
        })
        .await?;
        answer_frame(&compute, 0, &empty_batch(4, 1), true)?;
        assert_eq!(client.got(first).await?.command, Command::Result);
        let a = client.read(transaction, &subject).await?;
        let c = client.read(transaction, &subject).await?;
        client.write(transaction, &subject).await?;
        until("both reads", || compute.count(Command::ComputeRead) == 3).await?;
        let refused = cross_refusal(1, 2);
        answer_with(&compute, 1, &refused)?;
        let d = client.read(transaction, &subject).await?;
        let (want, detail) = (ErrorCode::CrossPartition, refused.detail());
        assert_eq!(client.refused(a).await?, (want, detail));
        assert_eq!(client.refused(c).await?, (want, detail));
        assert_eq!(client.refused(d).await?, (want, detail));
        client.write(transaction, &subject).await?;
        assert_eq!(client.refused(transaction).await?, (want, detail));
        assert_eq!(
            partition.count(Command::Write),
            0,
            "the Write went to no partition"
        );
        Ok(())
    })
}

#[then(
    "A, C and D are each answered code 23, the `detail` of A's, the Write is sent to no partition, and the next request on the transaction's stream is answered code 23 and ends it"
)]
fn a_read_s_code_23_answers_the_transaction_s_other_reads_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a transaction's first Write names subjects the frontend's map places in partitions 1 and 2"
)]
fn a_first_request_spreading_over_partitions_is_refused_before_any_bind(
    w: &mut Steps,
) -> TestResult {
    let options = Options {
        partitions: 2,
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let (one, two) = (stage.partition(1).await?, stage.partition(2).await?);
        let mut terms = TermTable::new();
        let changes = vec![
            insert_into(&mut terms, &stage.on(1)?),
            insert_into(&mut terms, &stage.on(2)?),
        ];
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        client.on(transaction, &Write::new(terms, changes)).await?;
        assert_eq!(
            client.refused(transaction).await?,
            (ErrorCode::CrossPartition, 1 | (2 << 32))
        );
        assert_eq!(one.count(Command::Bind) + two.count(Command::Bind), 0);
        Ok(())
    })
}

#[when(
    "a ConditionalWrite inserts at a subject of partition 2 and adds to a counter of a subject of partition 1"
)]
fn a_conditional_write_spreading_by_its_operation_is_refused(w: &mut Steps) -> TestResult {
    let options = Options {
        partitions: 2,
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let (one, two) = (stage.partition(1).await?, stage.partition(2).await?);
        let mut terms = TermTable::new();
        let change = insert_into(&mut terms, &stage.on(2)?);
        let counted = terms.iri(&stage.on(1)?);
        let predicate = terms.iri("http://ex.org/count");
        let operation = CommitOperation::CounterAdd {
            graph: None,
            subject: counted,
            predicate,
            delta: 1,
        };
        let mut client = stage.ann().await?;
        let write = client
            .send(&ConditionalWrite::new(
                0,
                terms,
                vec![change],
                vec![operation],
            ))
            .await?;
        assert_eq!(
            client.refused(write).await?,
            (ErrorCode::CrossPartition, 1 | (2 << 32))
        );
        let relayed = one.count(Command::ConditionalWrite) + two.count(Command::ConditionalWrite);
        assert_eq!(relayed, 0);
        Ok(())
    })
}

#[then("it is refused with code 23, `detail` partitions 1 and 2, and no partition receives it")]
fn a_conditional_write_spreading_by_its_operation_is_refused_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[then(
    "it is refused with code 23 on the transaction's stream, `detail` partitions 1 and 2, no partition receives a Bind, and the transaction ends"
)]
fn a_first_request_spreading_over_partitions_is_refused_before_any_bind_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a transaction's first read takes six frames at `initial_credit` 4, and after its first batch the client sends a second read and waits on it"
)]
fn a_first_read_holds_a_later_read_only_until_its_first_frame(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let subject = stage.on(1)?;
        let _partition = stage.partition(1).await?;
        let compute = stage.compute_with(|_| Answer::Open).await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        let first = client.read(transaction, &subject).await?;
        let second = client.read(transaction, &subject).await?;
        until("the first read", || {
            compute.count(Command::ComputeRead) == 1
        })
        .await?;
        pause(20).await;
        assert_eq!(
            compute.count(Command::ComputeRead),
            1,
            "the second waits for a frame"
        );
        answer_frame(&compute, 0, &empty_batch(4, 1), false)?;
        until("the second read", || {
            compute.count(Command::ComputeRead) == 2
        })
        .await?;
        answer_frame(&compute, 1, &empty_batch(4, 1), true)?;
        assert_eq!(
            client.got(second).await?.command,
            Command::Result,
            "its rows"
        );
        assert_eq!(
            client.got(first).await?.command,
            Command::Result,
            "the first still open"
        );
        Ok(())
    })
}

#[then(
    "the frontend sends the second read to compute on the first read's first frame, and the client receives its rows with the first read still open"
)]
fn a_first_read_holds_a_later_read_only_until_its_first_frame_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("two compute processes have joined and a connection sends two reads")]
fn reads_alternate_between_two_compute_processes(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let (one, two) = (stage.compute().await?, stage.compute().await?);
        let mut client = stage.ann().await?;
        let transaction = client.begin_read_only(ReadLevel::Snapshot).await?;
        let first = client.read(transaction, OBJECT).await?;
        let second = client.read(transaction, OBJECT).await?;
        assert_eq!(client.got(first).await?.command, Command::Result);
        assert_eq!(client.got(second).await?.command, Command::Result);
        assert_eq!(one.count(Command::ComputeRead), 1);
        assert_eq!(two.count(Command::ComputeRead), 1);
        Ok(())
    })
}

#[then("each process receives one")]
fn reads_alternate_between_two_compute_processes_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "the connection to one of two compute processes closes under a read before any frame of it reached the client"
)]
fn a_closed_compute_process_is_skipped(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let closing = stage.compute_with(|_| Answer::Open).await?;
        let other = stage.compute().await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin_read_only(ReadLevel::Snapshot).await?;
        let read = client.read(transaction, OBJECT).await?;
        until("the read", || closing.count(Command::ComputeRead) == 1).await?;
        closing.close();
        assert_eq!(client.got(read).await?.command, Command::Result, "its rows");
        assert_eq!(other.count(Command::ComputeRead), 1);
        Ok(())
    })
}

#[then("the frontend sends the read to the other, and the client receives its rows")]
fn a_closed_compute_process_is_skipped_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("no connection to a compute process can be opened for a read")]
fn no_compute_process_can_be_reached(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let mut client = stage.ann().await?;
        let transaction = client.begin_read_only(ReadLevel::Snapshot).await?;
        let read = client.read(transaction, OBJECT).await?;
        assert_eq!(client.refused(read).await?.0, ErrorCode::Unavailable);
        let _open = client.begin_read_only(ReadLevel::Snapshot).await?;
        Ok(())
    })
}

#[then("the client receives code 17, and its connection stays open")]
fn no_compute_process_can_be_reached_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a compute process's connection closes after one frame of a read reached the client")]
fn a_read_is_never_resent_after_a_frame(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let closing = stage.compute_with(|_| Answer::Open).await?;
        let other = stage.compute().await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin_read_only(ReadLevel::Snapshot).await?;
        let read = client.read(transaction, OBJECT).await?;
        until("the read", || closing.count(Command::ComputeRead) == 1).await?;
        answer_frame(&closing, 0, &empty_batch(4, 1), false)?;
        assert_eq!(client.got(read).await?.command, Command::Result);
        closing.close();
        assert_eq!(client.refused(read).await?.0, ErrorCode::Unavailable);
        assert_eq!(other.count(Command::ComputeRead), 0);
        let _open = client.begin_read_only(ReadLevel::Snapshot).await?;
        Ok(())
    })
}

#[then(
    "the read is refused with code 17, no other compute process receives it, and the client's connection stays open"
)]
fn a_read_is_never_resent_after_a_frame_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a compute process sends a ComputeRead's stream a Result frame with the stream's credit spent, the frames its credit and a Credit allowed having reached the client and a second read being open on the connection"
)]
fn a_result_frame_past_its_credit_closes_the_role_connection(w: &mut Steps) -> TestResult {
    let options = Options {
        limits: Options::default().limits.with_initial_credit(1),
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let breaking = stage.compute_with(|_| Answer::Open).await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin_read_only(ReadLevel::Snapshot).await?;
        let first = client.read(transaction, OBJECT).await?;
        let second = client.read(transaction, OBJECT).await?;
        until("both reads", || breaking.count(Command::ComputeRead) == 2).await?;
        let other = stage.compute().await?;
        answer_frame(&breaking, 0, &empty_batch(4, 1), false)?;
        assert_eq!(client.got(first).await?.command, Command::Result);
        client.on(first, &Credit::new(1)).await?;
        until("the Credit relayed", || {
            breaking.count(Command::Credit) == 1
        })
        .await?;
        answer_frame(&breaking, 0, &empty_batch(4, 1), false)?;
        assert_eq!(client.got(first).await?.command, Command::Result);
        // The second read's credit lets this frame leave, and its header names the first read, whose credit is spent.
        let reads = breaking.frames(Command::ComputeRead);
        let (named, spending) = (
            reads.first().context("the first read")?,
            reads.get(1).context("the second read")?,
        );
        spending.answer_with(batch_frame(
            CLUSTER,
            named.stream(),
            &empty_batch(4, 1),
            false,
        )?);
        assert_eq!(client.refused(first).await?.0, ErrorCode::Unavailable);
        assert_eq!(
            client.got(second).await?.command,
            Command::Result,
            "its rows"
        );
        assert_eq!(other.count(Command::ComputeRead), 1);
        until("the role connection's end", || {
            breaking.connections_ended() == 1
        })
        .await?;
        Ok(())
    })
}

#[then(
    "the frontend closes the role connection, answers the first read code 17, and sends the second to the other compute process"
)]
fn a_result_frame_past_its_credit_closes_the_role_connection_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a compute process accepts a read of `deadline_ms` 50 and sends nothing, `scan_stall_ms` is 100, and the read's stream holds credit"
)]
fn a_hung_compute_process_answers_code_17(w: &mut Steps) -> TestResult {
    let options = Options {
        limits: stalling(100),
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let hung = stage.compute_with(|_| Answer::Open).await?;
        let other = stage.compute().await?;
        let mut client = stage.ann().await?;
        let begin = Begin::new(0, 0, true).with_deadline_ms(50);
        let transaction = client.send(&begin).await?;
        let _begun = client.got(transaction).await?;
        let read = client.read(transaction, OBJECT).await?;
        client.silent(read, 100).await?;
        assert_eq!(client.refused(read).await?.0, ErrorCode::Unavailable);
        until("the ComputeRead cancelled", || {
            hung.count(Command::Cancel) == 1
        })
        .await?;
        assert_eq!(other.count(Command::ComputeRead), 0);
        Ok(())
    })
}

#[then(
    "the frontend cancels the ComputeRead and answers the read with code 17 once 150 ms have passed, and no other compute process receives it"
)]
fn a_hung_compute_process_answers_code_17_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a read of `deadline_ms` 40,000 waits 35 s for a close, and compute sends its first frame once the close arrives"
)]
fn a_compute_process_waiting_within_the_deadline_is_not_hung(w: &mut Steps) -> TestResult {
    let options = Options {
        limits: stalling(100),
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        // The scenario's 40,000 ms and 35 s at a hundredth: the rule compares the wait with the deadline, whatever their size.
        let compute = stage.compute_with(|_| Answer::Open).await?;
        let mut client = stage.ann().await?;
        let begin = Begin::new(0, 0, true).with_deadline_ms(400);
        let transaction = client.send(&begin).await?;
        let _begun = client.got(transaction).await?;
        let read = client.read(transaction, OBJECT).await?;
        until("the read", || compute.count(Command::ComputeRead) == 1).await?;
        pause(350).await;
        answer_frame(&compute, 0, &empty_batch(4, 1), true)?;
        assert_eq!(client.got(read).await?.command, Command::Result, "its rows");
        Ok(())
    })
}

#[then("the client receives the rows")]
fn a_compute_process_waiting_within_the_deadline_is_not_hung_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a compute process sends a read's frames with gaps below `scan_stall_ms`, past the read's deadline"
)]
fn a_frame_restarts_a_hung_compute_s_timer(w: &mut Steps) -> TestResult {
    let options = Options {
        limits: stalling(100),
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let compute = stage.compute_with(|_| Answer::Open).await?;
        let mut client = stage.ann().await?;
        let begin = Begin::new(0, 0, true).with_deadline_ms(10);
        let transaction = client.send(&begin).await?;
        let _begun = client.got(transaction).await?;
        let read = client.read(transaction, OBJECT).await?;
        until("the read", || compute.count(Command::ComputeRead) == 1).await?;
        for last in [false, false, false, true] {
            pause(60).await;
            answer_frame(&compute, 0, &empty_batch(4, 1), last)?;
            assert_eq!(
                client.got(read).await?.command,
                Command::Result,
                "no code 17"
            );
        }
        assert_eq!(compute.count(Command::Cancel), 0);
        Ok(())
    })
}

#[then("the read goes on to its `LAST`")]
fn a_frame_restarts_a_hung_compute_s_timer_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a read's stream has spent its credit, and its client takes no batch for longer than `scan_stall_ms`"
)]
fn a_stream_without_credit_holds_off_a_hung_compute_s_timer(w: &mut Steps) -> TestResult {
    let options = Options {
        limits: stalling(50).with_initial_credit(1),
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let compute = stage.compute_with(|_| Answer::Open).await?;
        let mut client = stage.ann().await?;
        let begin = Begin::new(0, 0, true).with_deadline_ms(10);
        let transaction = client.send(&begin).await?;
        let _begun = client.got(transaction).await?;
        let read = client.read(transaction, OBJECT).await?;
        until("the read", || compute.count(Command::ComputeRead) == 1).await?;
        answer_frame(&compute, 0, &empty_batch(4, 1), false)?;
        assert_eq!(client.got(read).await?.command, Command::Result);
        client.silent(read, 200).await?;
        assert_eq!(compute.count(Command::Cancel), 0, "no credit, no timer");
        Ok(())
    })
}

#[then("the frontend does not answer it with code 17 on that account")]
fn a_stream_without_credit_holds_off_a_hung_compute_s_timer_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a partition stops on an invariant violation and a client writes to it")]
fn a_partition_s_invariant_violation_answers_unavailable(w: &mut Steps) -> TestResult {
    let options = Options {
        partitions: 2,
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let _stopped = stage
            .partition_with(1, |frame| {
                if frame.command() == Command::Write {
                    return Answer::Refuse(ErrorReply::new(
                        ErrorCode::Unavailable,
                        "stopped on an invariant",
                    ));
                }
                partition_answer(frame, CLUSTER, BIND_VERSION)
            })
            .await?;
        let _other = stage.partition(2).await?;
        let mut client = stage.ann().await?;
        let broken = client.begin(0).await?;
        client.write(broken, &stage.on(1)?).await?;
        assert_eq!(client.refused(broken).await?.0, ErrorCode::Unavailable);
        let served = client.begin(0).await?;
        client.write(served, &stage.on(2)?).await?;
        client.commit(served).await?;
        assert_eq!(client.got(served).await?.command, Command::Commit);
        Ok(())
    })
}

#[then("the client receives `Unavailable`, and requests to other partitions still succeed")]
fn a_partition_s_invariant_violation_answers_unavailable_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "the connection between the frontend and a storage process closes under a Write and a Commit"
)]
fn a_role_connection_closing_under_a_request_answers_code_17(w: &mut Steps) -> TestResult {
    let options = Options {
        partitions: 2,
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let subject = stage.on(1)?;
        let holding = subject.clone();
        let closing = stage
            .partition_with(1, move |frame| {
                let holds = frame.command() == Command::Commit
                    || (frame.command() == Command::Write
                        && String::from_utf8_lossy(frame.bytes()).contains(&holding));
                if holds {
                    return Answer::Open;
                }
                partition_answer(frame, CLUSTER, BIND_VERSION)
            })
            .await?;
        let _other = stage.partition(2).await?;
        let mut client = stage.ann().await?;
        let writing = client.begin(0).await?;
        client.write(writing, &subject).await?;
        let committing = client.begin(0).await?;
        client.write(committing, &stage.on_tagged(1, "b")?).await?;
        client.commit(committing).await?;
        until("the Write and the Commit", || {
            closing.count(Command::Commit) == 1 && closing.count(Command::Write) == 2
        })
        .await?;
        closing.close();
        assert_eq!(client.refused(writing).await?.0, ErrorCode::Unavailable);
        assert_eq!(client.refused(committing).await?.0, ErrorCode::Unavailable);
        let served = client.begin(0).await?;
        client.write(served, &stage.on(2)?).await?;
        client.commit(served).await?;
        assert_eq!(client.got(served).await?.command, Command::Commit);
        Ok(())
    })
}

#[then(
    "the client receives code 17 for each, the Commit is in doubt, and requests to other partitions still succeed"
)]
fn a_role_connection_closing_under_a_request_answers_code_17_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a client declares a vector index on `embedding`")]
fn a_vector_index_is_refused(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let before = stage
            .owner
            .schema_id(0)
            .await
            .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        let declared =
            DeclaredPredicate::new("http://ex.org/embedding").with_vector_index("hnsw", Vec::new());
        client.on(transaction, &declaring(declared)).await?;
        let got = client.got(transaction).await?;
        assert_eq!(got.code, Some(ErrorCode::Refused));
        assert!(
            got.message.contains("embedding") && got.message.contains("stage 8"),
            "{}",
            got.message
        );
        let after = stage
            .owner
            .schema_id(0)
            .await
            .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
        assert_eq!(before, after, "the catalog is unchanged");
        Ok(())
    })
}

#[then("the declaration is refused with an error naming `embedding`, and the catalog is unchanged")]
fn a_vector_index_is_refused_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "an authenticated principal sends an Install naming tenant 0, a Query in an open transaction, and a Prepare of `language` 1"
)]
fn a_command_of_a_later_stage_is_refused_on_its_stream(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let mut client = stage.ann().await?;
        let install = client
            .send(&Install::new(
                0,
                0,
                vec![("a".to_string(), "b".to_string())],
            ))
            .await?;
        let got = client.got(install).await?;
        assert_eq!(got.code, Some(ErrorCode::Refused));
        assert!(
            got.message.contains("Install") && got.message.contains("stage 6"),
            "{}",
            got.message
        );
        let transaction = client.begin(0).await?;
        let query = client
            .send(&Query::new(
                transaction,
                Language::Gql,
                "MATCH",
                TermTable::new(),
            ))
            .await?;
        let got_2 = client.got(query).await?;
        assert_eq!(got_2.code, Some(ErrorCode::Refused));
        assert!(
            got_2.message.contains("Query") && got_2.message.contains("stage 7"),
            "{}",
            got_2.message
        );
        client.commit(transaction).await?;
        assert_eq!(
            client.got(transaction).await?.command,
            Command::Commit,
            "its transaction stays open"
        );
        let prepare = client
            .send(&Prepare::of_text(
                0,
                Language::Gql,
                "MATCH",
                TermTable::new(),
                Vec::new(),
                Vec::new(),
            ))
            .await?;
        let got_3 = client.got(prepare).await?;
        assert_eq!(got_3.code, Some(ErrorCode::Refused));
        assert!(
            got_3.message.contains("Prepare") && got_3.message.contains("stage 7"),
            "{}",
            got_3.message
        );
        let _open = client.begin(0).await?;
        Ok(())
    })
}

#[then(
    "each is refused with code 7 on its stream, the message naming the command and its stage, the Query's transaction stays open, and the connection stays open"
)]
fn a_command_of_a_later_stage_is_refused_on_its_stream_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("an authenticated principal sends a Begin naming tenant 0")]
fn the_gate_allows_an_authenticated_principal_in_this_stage(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let mut client = stage.ann().await?;
        let _answered = client.begin(0).await?;
        Ok(())
    })
}

#[then("the gate passes it, and the Begin is answered")]
fn the_gate_allows_an_authenticated_principal_in_this_stage_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "an authenticated principal sends a Begin, a Prepare, a Describe, an Install, a Drop, a DirectWrite and a ConditionalWrite, each naming tenant 9, which the metadata group does not hold"
)]
fn an_unknown_tenant_is_refused_after_the_gate(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let partition = stage.partition(1).await?;
        let mut client = stage.ann().await?;
        let mut streams = vec![client.send(&Begin::new(9, 0, false)).await?];
        streams.push(client.send(&prepared_in(9, OBJECT)).await?);
        streams.push(client.send(&Describe::request(9)).await?);
        streams.push(
            client
                .send(&Install::new(
                    9,
                    0,
                    vec![("a".to_string(), "b".to_string())],
                ))
                .await?,
        );
        streams.push(client.send(&DropCommand::tenant_data(9)).await?);
        let mut terms = TermTable::new();
        let change = insert_into(&mut terms, OBJECT);
        streams.push(
            client
                .send(&DirectWrite::new(9, terms.clone(), vec![change.clone()]))
                .await?,
        );
        let operation = CommitOperation::CounterAdd {
            graph: None,
            subject: 0,
            predicate: 1,
            delta: 1,
        };
        streams.push(
            client
                .send(&ConditionalWrite::new(
                    9,
                    terms,
                    vec![change],
                    vec![operation],
                ))
                .await?,
        );
        for stream in streams {
            let got = client.got(stream).await?;
            assert_eq!((got.code, got.detail), (Some(ErrorCode::Refused), 9));
            assert!(got.message.contains('9'), "{}", got.message);
        }
        assert!(partition.commands(&[]).is_empty(), "no other role sees it");
        Ok(())
    })
}

#[then(
    "each is refused with code 7, `detail` 9, the message naming tenant 9, and no other role sees it"
)]
fn an_unknown_tenant_is_refused_after_the_gate_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "the gate, built over an authorizer denying `ann` every action, is asked for tenant 0, which exists, and tenant 9, which does not"
)]
fn a_principal_the_gate_denies_learns_nothing_of_a_number(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        drop(stage);
        let stage_2 = Stage::start_gated(Options::default(), Deny).await?;
        let mut client = stage_2.ann().await?;
        let there = client.send(&Begin::new(0, 0, false)).await?;
        let there = client.got(there).await?;
        let not = client.send(&Begin::new(9, 0, false)).await?;
        let not = client.got(not).await?;
        assert_eq!(there.code, Some(ErrorCode::PermissionDenied));
        assert_eq!(there.detail, Action::Read.number());
        assert_eq!(
            (there.code, there.detail, &there.message),
            (not.code, not.detail, &not.message)
        );
        Ok(())
    })
}

#[then(
    "each request is refused with code 21, `detail` its action, and the two refusals differ in nothing else"
)]
fn a_principal_the_gate_denies_learns_nothing_of_a_number_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("the gate is asked of an action or a scope it does not know")]
fn the_gate_fails_closed(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let gate = TenantGate::new(EveryAuthenticated);
        let denied = gate
            .check(1, 99, GateScope::Tenant(0))
            .err()
            .context("an unknown action")?;
        assert_eq!(
            (denied.code(), denied.detail()),
            (ErrorCode::PermissionDenied, 99)
        );
        let denied_2 = gate
            .check(1, Action::Read.number(), GateScope::Cluster)
            .err()
            .context("an unknown scope")?;
        assert_eq!(denied_2.code(), ErrorCode::PermissionDenied);
        let denied_3 = gate
            .check(1, Action::Operate.number(), GateScope::Tenant(0))
            .err()
            .context("an action of no tenant")?;
        assert_eq!(denied_3.code(), ErrorCode::PermissionDenied);
        drop(stage);
        Ok(())
    })
}

#[then("it denies, and the request is refused with code 21")]
fn the_gate_fails_closed_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("the gate denies a DirectWrite")]
fn a_denied_request_reaches_no_role(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        drop(stage);
        let stage_2 = Stage::start_gated(Options::default(), Deny).await?;
        let partition = stage_2.partition(1).await?;
        let mut client = stage_2.ann().await?;
        let mut terms = TermTable::new();
        let change = insert_into(&mut terms, OBJECT);
        let write = client
            .send(&DirectWrite::new(0, terms, vec![change]))
            .await?;
        assert_eq!(
            client.refused(write).await?,
            (ErrorCode::PermissionDenied, Action::Write.number())
        );
        assert!(partition.commands(&[]).is_empty(), "no partition sees it");
        Ok(())
    })
}

#[then("it is refused with code 21, `detail` 2, and no partition sees it")]
fn a_denied_request_reaches_no_role_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "`held_bytes` is 1,024, a read of a transaction is open, and the client sends Writes that the read holds back past 1,024 bytes"
)]
fn a_write_past_held_bytes_is_refused(w: &mut Steps) -> TestResult {
    let options = Options {
        limits: EndpointLimits::new(1_024, Duration::from_secs(30)),
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let subject = stage.on(1)?;
        let _partition = stage.partition(1).await?;
        let compute = stage.compute_with(|_| Answer::Open).await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        let _read = client.read(transaction, &subject).await?;
        until("the read", || compute.count(Command::ComputeRead) == 1).await?;
        let (mut sent, frame) = (
            0,
            write_of(&subject)
                .frame(Header::new(CLUSTER, Command::Write))?
                .len(),
        );
        while sent <= 1_024 {
            client.write(transaction, &subject).await?;
            sent += frame;
        }
        assert_eq!(
            client.refused(transaction).await?,
            (ErrorCode::StreamLimit, 1_024)
        );
        Ok(())
    })
}

#[then(
    "the Write past the bound is refused with code 15, `detail` 1,024, on the transaction's stream, and the transaction ends"
)]
fn a_write_past_held_bytes_is_refused_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a read holds Writes back, and the client grants the read credit")]
fn credit_still_drains_a_read_under_held_writes(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let subject = stage.on(1)?;
        let partition = stage.partition(1).await?;
        let compute = stage.compute_with(|_| Answer::Open).await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        let read = client.read(transaction, &subject).await?;
        until("the read", || compute.count(Command::ComputeRead) == 1).await?;
        client.write(transaction, &subject).await?;
        client.on(read, &Credit::new(1)).await?;
        until("the Credit relayed", || compute.count(Command::Credit) == 1).await?;
        assert_eq!(
            partition.count(Command::Write),
            0,
            "the read still holds it"
        );
        answer_frame(&compute, 0, &empty_batch(4, 1), true)?;
        assert_eq!(client.got(read).await?.command, Command::Result);
        until("the held Write", || partition.count(Command::Write) == 1).await?;
        Ok(())
    })
}

#[then(
    "the frontend reads the Credit and relays it, the read reaches its `LAST`, and the held Writes are then sent"
)]
fn credit_still_drains_a_read_under_held_writes_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a read-write transaction begins and sends nothing for `transaction_ms`")]
fn an_unbound_transaction_passes_its_limit(w: &mut Steps) -> TestResult {
    let options = Options {
        limits: limited(50),
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let partition = stage.partition(1).await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        assert_eq!(
            client.refused(transaction).await?,
            (ErrorCode::PastLimit, 50)
        );
        assert!(partition.commands(&[]).is_empty(), "no partition saw it");
        Ok(())
    })
}

#[then("the frontend answers code 4 on its stream, with no partition having seen it")]
fn an_unbound_transaction_passes_its_limit_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a read-write transaction bound to partition 1 sends nothing for `transaction_ms` from its Begin"
)]
fn a_bound_transaction_passes_its_limit(w: &mut Steps) -> TestResult {
    let options = Options {
        limits: limited(100),
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let partition = stage.partition(1).await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        client.write(transaction, &stage.on(1)?).await?;
        assert_eq!(
            client.refused(transaction).await?,
            (ErrorCode::PastLimit, 100)
        );
        until("the Bind stream cancelled", || {
            partition.count(Command::Cancel) == 1
        })
        .await?;
        Ok(())
    })
}

#[then(
    "the frontend cancels the Bind stream, which aborts it at the partition, and answers code 4 on its stream"
)]
fn a_bound_transaction_passes_its_limit_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a read of a transaction has spent its credit, the client sends a Write and a second read, waits on the second read, and grants no credit for `transaction_ms`"
)]
fn a_read_holding_a_write_back_is_named_at_the_limit(w: &mut Steps) -> TestResult {
    let options = Options {
        limits: limited(150).with_initial_credit(1),
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let subject = stage.on(1)?;
        let _partition = stage.partition(1).await?;
        let compute = stage.compute_with(|_| Answer::Open).await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        let first = client.read(transaction, &subject).await?;
        until("the read", || compute.count(Command::ComputeRead) == 1).await?;
        answer_frame(&compute, 0, &empty_batch(4, 1), false)?;
        assert_eq!(client.got(first).await?.command, Command::Result);
        client.write(transaction, &subject).await?;
        let second = client.read(transaction, &subject).await?;
        let got = client.got(second).await?;
        assert_eq!(got.code, Some(ErrorCode::PastLimit));
        let named = format!("stream {first}");
        assert!(
            got.message.contains(&named) && got.message.contains("no credit"),
            "{}",
            got.message
        );
        Ok(())
    })
}

#[then(
    "the frontend answers code 4, the message naming the first read's stream and that it had no credit"
)]
fn a_read_holding_a_write_back_is_named_at_the_limit_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a transaction's Commit has been relayed, and `transaction_ms` passes before its reply")]
fn a_relayed_commit_is_not_timed(w: &mut Steps) -> TestResult {
    let options = Options {
        limits: limited(50),
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let partition = stage
            .partition_with(1, |frame| {
                if frame.command() == Command::Commit {
                    return Answer::Open;
                }
                partition_answer(frame, CLUSTER, BIND_VERSION)
            })
            .await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        client.write(transaction, &stage.on(1)?).await?;
        client.commit(transaction).await?;
        until("the Commit", || partition.count(Command::Commit) == 1).await?;
        pause(100).await;
        let held = partition.frames(Command::Commit);
        let held = held.first().context("the Commit")?;
        held.answer_with(last_frame(
            CLUSTER,
            held.stream(),
            &Commit::new([0; 16], 0, BIND_VERSION),
        )?);
        assert_eq!(
            client.got(transaction).await?.command,
            Command::Commit,
            "the Commit's reply"
        );
        Ok(())
    })
}

#[then("the client receives the Commit's reply")]
fn a_relayed_commit_is_not_timed_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a client begins a read-write transaction and sends Commit through its session with nothing else"
)]
fn an_empty_transaction_commits_at_the_frontend(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let partition = stage.partition(1).await?;
        let mut client = stage.ann().await?;
        let session = client.register().await?;
        let transaction = client.begin(0).await?;
        client.on(transaction, &Commit::new(session, 1, 0)).await?;
        let got = client.got(transaction).await?;
        let view = view_frame(&got.bytes)?;
        let reply = CommitView::from_frame(&view)?;
        assert_eq!((reply.commit_version(), reply.result_count()), (0, 0));
        assert!(reply.token().is_none(), "the token zero");
        assert!(partition.commands(&[]).is_empty(), "no partition sees it");
        Ok(())
    })
}

#[then(
    "the frontend answers `commit_version` 0, the token zero and no results, and no partition sees it"
)]
fn an_empty_transaction_commits_at_the_frontend_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("that Commit's reply is lost, and the client sends it again")]
fn a_resent_empty_commit_answers_not_committed(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let _partition = stage.partition(1).await?;
        let mut client = stage.ann().await?;
        let session = client.register().await?;
        let transaction = client.begin(0).await?;
        client.on(transaction, &Commit::new(session, 1, 0)).await?;
        let _reply = client.got(transaction).await?;
        let resent = client.send(&Commit::new(session, 1, 0)).await?;
        assert_eq!(client.refused(resent).await?, (ErrorCode::NotCommitted, 1));
        Ok(())
    })
}

#[then("every partition answers 18, and the client receives code 18")]
fn a_resent_empty_commit_answers_not_committed_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a client sends a Write the frontend refuses and, before reading its answer, a second Write of the same transaction"
)]
fn a_transactions_first_refusal_is_the_one_its_client_sees(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let _partition = stage.partition(1).await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        let mut terms = TermTable::new();
        let literal = terms.intern(RequestTerm::Literal {
            lexical: "x".to_string(),
            datatype: None,
        })?;
        let change = WriteChange::Insert {
            graph: None,
            subject: literal,
            predicate: terms.iri(PREDICATE),
            object: terms.iri(OBJECT),
        };
        client
            .on(transaction, &Write::new(terms, vec![change]))
            .await?;
        client.write(transaction, &stage.on(1)?).await?;
        let got = client.got(transaction).await?;
        assert_eq!(
            (got.code, got.detail),
            (Some(ErrorCode::Refused), u64::from(literal)),
            "{}",
            got.message
        );
        client.silent(transaction, 50).await?;
        Ok(())
    })
}

#[then(
    "the client receives the first Write's refusal on the transaction's stream, and nothing after it"
)]
fn a_transactions_first_refusal_is_the_one_its_client_sees_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a monotonic-fast transaction's read answered at epoch 12 and another at epoch 9, and a third is sent after both"
)]
fn concurrent_monotonic_fast_reads_take_the_highest_epoch_answered(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let subject = stage.on(1)?;
        let compute = stage.compute_with(|_| Answer::Open).await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin_read_only(ReadLevel::MonotonicFast).await?;
        let first = client.read(transaction, &subject).await?;
        let second = client.read(transaction, &subject).await?;
        until("both reads", || compute.count(Command::ComputeRead) == 2).await?;
        let answered = [(0, 12), (1, 9)];
        for (index, epoch) in answered {
            answer_frame(
                &compute,
                index,
                &empty_batch(4, 1).with_read_at(epoch, 1),
                true,
            )?;
        }
        for read in [first, second] {
            assert_eq!(client.got(read).await?.command, Command::Result);
        }
        let _third = client.read(transaction, &subject).await?;
        until("the third read", || {
            compute.count(Command::ComputeRead) == 3
        })
        .await?;
        let reads = compute.frames(Command::ComputeRead);
        let third = reads.get(2).context("the third ComputeRead")?;
        let view = view_frame(third.bytes())?;
        let highest = answered.iter().map(|(_, epoch)| *epoch).max();
        assert_eq!(
            Some(ComputeReadView::from_frame(&view)?.epoch()),
            highest,
            "the third read's floor"
        );
        Ok(())
    })
}

#[then("the third read's floor is 12")]
fn concurrent_monotonic_fast_reads_take_the_highest_epoch_answered_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}
