/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What a client connection has on its way to a role: its bound stops its reads and refuses nothing (`PROTOCOL.md` §11.12).

use std::time::Duration;

use cucumber::{then, when};
use ultragraphdb_common_frontend::{
    DoubleFrame, EndpointLimits, RoleDouble, last_frame, partition_answer,
};
use ultragraphdb_common_role::Answer;
use ultragraphdb_protocol_wire::{
    Abort, Begin, BodyBuilder, Command, Commit, DirectWrite, ErrorCode, Header, RequestTerm,
    TermTable, WriteChange,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::Steps;
use crate::endpoint_stage::{
    BIND_VERSION, CLUSTER, Client, OBJECT, Options, PREDICATE, Stage, until, write_of,
};
use crate::endpoint_steps_requests::run;

const BOUND: u64 = 4_096;
const STREAMS: usize = 4;
/// How long a scenario gives a frame that must not be read to arrive all the same.
const SILENCE_MS: u64 = 200;
/// A scenario's `transaction_ms`: long enough to send what it sends.
const LIMIT_MS: u32 = 800;
/// Past what a socket's buffers on both sides hold on any host here, so the last one-frame writes stay in the frontend's queue.
const LARGE_WRITES: usize = 120;
const LARGE_BYTES: usize = 200_000;

/// Limits with `held_bytes` of [`BOUND`].
fn bounded() -> EndpointLimits {
    EndpointLimits::new(BOUND, Duration::from_secs(30))
}

/// A partition answering all but a Commit, whose reply a step sends.
fn holding_commits(frame: &DoubleFrame) -> Answer {
    if frame.command() == Command::Commit {
        return Answer::Open;
    }
    partition_answer(frame, CLUSTER, BIND_VERSION)
}

/// Answers `frame`, a Commit through no session, as committed.
fn commit_answered(frame: &DoubleFrame) -> TestResult {
    frame.answer_with(last_frame(
        CLUSTER,
        frame.stream(),
        &Commit::new([0; 16], 0, BIND_VERSION),
    )?);
    Ok(())
}

/// `transactions`' streams, each bound to partition 1 with a first Write: what each stream sent, in order.
async fn bound_on_one(
    stage: &Stage,
    client: &mut Client,
    transactions: usize,
) -> TestResult<Vec<(u64, Vec<String>)>> {
    let mut streams = Vec::with_capacity(transactions);
    for at in 0..transactions {
        let transaction = client.begin(0).await?;
        let subject = stage.on_tagged(1, &format!("{at}-0-"))?;
        client.write(transaction, &subject).await?;
        streams.push((transaction, vec![subject]));
    }
    Ok(streams)
}

/// Sends Writes for partition 1 round the `streams` until `past` bytes have gone.
async fn pipeline(
    stage: &Stage,
    client: &mut Client,
    streams: &mut [(u64, Vec<String>)],
    past: usize,
) -> TestResult<usize> {
    let (mut sent, mut largest, mut round) = (0, 0, 1);
    while sent <= past {
        for (at, (transaction, subjects)) in streams.iter_mut().enumerate() {
            let subject = stage.on_tagged(1, &format!("{at}-{round}-"))?;
            let frame = write_of(&subject)
                .frame(Header::new(CLUSTER, Command::Write))?
                .len();
            client.write(*transaction, &subject).await?;
            subjects.push(subject);
            sent += frame;
            largest = largest.max(frame);
        }
        round += 1;
    }
    Ok(largest)
}

/// That each stream's Writes reached `partition` whole and in the order sent, its Binds in the order of `streams`.
fn in_their_order(partition: &RoleDouble, streams: &[(u64, Vec<String>)]) -> TestResult {
    let writes = partition.frames(Command::Write);
    let binds = partition.frames(Command::Bind);
    assert_eq!(binds.len(), streams.len());
    for (bind, (_, subjects)) in binds.iter().zip(streams) {
        let reached: Vec<_> = writes
            .iter()
            .filter(|write| write.stream() == bind.stream())
            .collect();
        assert_eq!(reached.len(), subjects.len(), "every Write reached it");
        for (write, subject) in reached.iter().zip(subjects) {
            assert!(
                String::from_utf8_lossy(write.bytes()).contains(subject.as_str()),
                "{subject} is out of its order"
            );
        }
    }
    Ok(())
}

#[when(
    "`held_bytes` is 4,096, partition 1 stops reading, a client pipelines Writes for partition 1 on four streams past the bound while its Commit on partition 2 waits for its reply, and sends a Begin behind them"
)]
fn a_client_past_its_bound_stops_being_read(w: &mut Steps) -> TestResult {
    let options = Options {
        partitions: 2,
        limits: bounded(),
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let stalled = stage.partition(1).await?;
        let other = stage.partition_with(2, holding_commits).await?;
        let mut client = stage.ann().await?;
        let waiting = client.begin(0).await?;
        client.write(waiting, &stage.on(2)?).await?;
        client.commit(waiting).await?;
        until("the Commit that waits", || {
            other.count(Command::Commit) == 1
        })
        .await?;
        let mut streams = bound_on_one(&stage, &mut client, STREAMS).await?;
        until("each first Write", || {
            stalled.count(Command::Write) == STREAMS
        })
        .await?;
        stalled.stop_reading();
        let frame = pipeline(&stage, &mut client, &mut streams, 4 * BOUND as usize).await?;
        let behind = client.send(&Begin::new(0, 0, false)).await?;

        client.silent(behind, SILENCE_MS).await?;
        let counted = stage.on_way(0);
        assert!(
            counted >= BOUND as usize && counted <= BOUND as usize + frame,
            "{counted} bytes counted against a bound of {BOUND} and a frame of {frame}"
        );
        let held = other.frames(Command::Commit);
        commit_answered(held.first().context("the Commit that waits")?)?;
        assert_eq!(client.got(waiting).await?.command, Command::Commit);

        let mut another = stage.as_principal("bob").await?;
        let theirs = another.begin(0).await?;
        another
            .write(theirs, &stage.on_tagged(2, "theirs")?)
            .await?;
        another.commit(theirs).await?;
        until("the other client's Commit", || {
            other.count(Command::Commit) == 2
        })
        .await?;
        let answers = other.frames(Command::Commit);
        commit_answered(answers.last().context("the other client's Commit")?)?;
        assert_eq!(another.got(theirs).await?.command, Command::Commit);
        assert_eq!(
            stage.on_way(0),
            counted,
            "its frames still wait on partition 1"
        );

        stalled.read_again();
        assert_eq!(client.got(behind).await?.command, Command::Begin);
        for (transaction, _) in &streams {
            client.commit(*transaction).await?;
        }
        for (transaction, _) in &streams {
            assert_eq!(client.got(*transaction).await?.command, Command::Commit);
        }
        in_their_order(&stalled, &streams)
    })
}

#[then(
    "the frontend counts at most the bound and one frame for it, reads no further frame of it, and sends it the reply of its Commit, another client commits on partition 2, and when partition 1 reads again the Begin is answered, each stream's Writes reach partition 1 in the order sent, and each Commit is answered"
)]
fn a_client_past_its_bound_stops_being_read_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "`held_bytes` is 4,096, a partition stops reading, and a client pipelines Writes for it past the bound and passes `transaction_ms`, a Begin behind them"
)]
fn a_limit_ends_what_a_stopped_partition_holds_back(w: &mut Steps) -> TestResult {
    let options = Options {
        limits: bounded().with_transaction_ms(LIMIT_MS),
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let stalled = stage.partition(1).await?;
        let mut client = stage.ann().await?;
        let mut streams = bound_on_one(&stage, &mut client, STREAMS).await?;
        until("each first Write", || {
            stalled.count(Command::Write) == STREAMS
        })
        .await?;
        stalled.stop_reading();
        let _frame = pipeline(&stage, &mut client, &mut streams, 2 * BOUND as usize).await?;
        let behind = client.send(&Begin::new(0, 0, false)).await?;
        client.silent(behind, SILENCE_MS).await?;
        assert!(stage.on_way(0) >= BOUND as usize, "the count met its bound");
        for (transaction, _) in &streams {
            assert_eq!(
                client.refused(*transaction).await?,
                (ErrorCode::PastLimit, u64::from(LIMIT_MS))
            );
        }
        assert_eq!(client.got(behind).await?.command, Command::Begin);
        assert_eq!(stage.on_way(0), 0, "the limit let go of what it held");
        stalled.read_again();
        Ok(())
    })
}

#[then(
    "each transaction is answered code 4, the frontend counts nothing for the client, and the Begin is answered while the partition still reads nothing"
)]
fn a_limit_ends_what_a_stopped_partition_holds_back_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

/// A one-frame write of about [`LARGE_BYTES`] in tenant 0, of `subject`.
fn large_write(subject: &str) -> TestResult<DirectWrite> {
    let mut terms = TermTable::new();
    let change = WriteChange::Insert {
        graph: None,
        subject: terms.iri(subject),
        predicate: terms.iri(PREDICATE),
        object: terms.intern(RequestTerm::Literal {
            lexical: OBJECT.repeat(LARGE_BYTES / OBJECT.len()),
            datatype: None,
        })?,
    };
    Ok(DirectWrite::new(0, terms, vec![change]))
}

#[when(
    "a partition stops reading with its socket full, a client's one-frame writes wait behind it past `transaction_ms`, and another client binds and aborts behind them"
)]
fn what_never_left_is_taken_back(w: &mut Steps) -> TestResult {
    let options = Options {
        limits: EndpointLimits::new(1 << 28, Duration::from_secs(30)).with_transaction_ms(LIMIT_MS),
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let subject = stage.on(1)?;
        let stalled = stage.partition(1).await?;
        let mut client = stage.ann().await?;
        let mut another = stage.as_principal("bob").await?;
        let first = client.send(&large_write(&subject)?).await?;
        assert_eq!(client.got(first).await?.command, Command::Commit);
        stalled.stop_reading();
        let mut writes = Vec::with_capacity(LARGE_WRITES);
        for _ in 0..LARGE_WRITES {
            writes.push(client.send(&large_write(&subject)?).await?);
        }
        let aborted = another.begin(0).await?;
        another.write(aborted, &subject).await?;
        another.on(aborted, &Abort).await?;

        let last = writes.last().copied().context("the last one-frame write")?;
        assert_eq!(
            client.refused(last).await?,
            (ErrorCode::PastLimit, u64::from(LIMIT_MS)),
            "the last never left"
        );
        until("the client's count to fall", || stage.on_way(0) == 0).await?;
        stalled.read_again();
        let mut refused = 1;
        for write in &writes {
            if *write == last {
                continue;
            }
            let answer = client.got(*write).await?;
            match answer.code {
                Some(code) => {
                    assert_eq!(code, ErrorCode::PastLimit);
                    refused += 1;
                }
                None => assert_eq!(answer.command, Command::Commit),
            }
        }
        // A frame of its own behind the rest, so the partition has read all it was sent.
        let behind = client.send(&large_write(&subject)?).await?;
        assert_eq!(client.got(behind).await?.command, Command::Commit);
        assert_eq!(
            stalled.count(Command::DirectWrite),
            LARGE_WRITES + 2 - refused,
            "none of the {refused} refused reached it"
        );
        assert_eq!(stalled.count(Command::Bind), 0, "no Bind left");
        assert_eq!(stalled.count(Command::Cancel), 0, "no Cancel left");
        Ok(())
    })
}

#[then(
    "each one-frame write that never left is answered code 4, the frontend counts nothing for the client, and when the partition reads again it sees none of those, no Bind and no Cancel, and answers the ones that left"
)]
fn what_never_left_is_taken_back_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}
