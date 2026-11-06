/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The scenarios of a full registry and the sessions it keeps: a session with a commit in flight stays registered, and a
//! Register finding no other session to evict is refused as busy.

use std::cell::Cell;
use std::rc::Rc;

use cucumber::{then, when};
use ultragraphdb_common_frontend::{DoubleFrame, RoleDouble, last_frame, partition_answer};
use ultragraphdb_common_role::Answer;
use ultragraphdb_protocol_wire::{
    BodyView, Cancel, Command, Commit, CommitView, DirectWrite, ErrorCode, Register, RegisterView,
    TermTable,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::Steps;
use crate::endpoint_stage::{
    BIND_VERSION, CLUSTER, Client, Options, Stage, insert_into, pause, until, view_frame,
};
use crate::endpoint_steps_requests::run;

/// The number of tries a step makes for what the frontend does after an answer no client sees, a few milliseconds apart.
const TRIES: u32 = 500;

/// A partition holding every Commit and DirectWrite unanswered until the scenario answers it, or until `answering` is set.
async fn holding_commits(stage: &Stage, answering: Rc<Cell<bool>>) -> TestResult<RoleDouble> {
    stage
        .partition_with(1, move |frame: &DoubleFrame| {
            let held = matches!(frame.command(), Command::Commit | Command::DirectWrite);
            if held && !answering.get() {
                return Answer::Open;
            }
            partition_answer(frame, CLUSTER, BIND_VERSION)
        })
        .await
}

/// Registers a session once one is free to evict, within [`TRIES`]: a Register refused with code 14 is sent again.
async fn register_once_free(client: &mut Client) -> TestResult<[u8; 16]> {
    for _ in 0..TRIES {
        let register = client.send(&Register::new([0; 16])).await?;
        let got = client.got(register).await?;
        match got.code {
            None => return Ok(RegisterView::from_frame(&view_frame(&got.bytes)?)?.session()),
            Some(ErrorCode::SessionBusy) => pause(2).await,
            Some(code) => {
                return Err(TestError::new(format!(
                    "the Register was refused with {code:?}"
                )));
            }
        }
    }
    Err(TestError::new(
        "no session came free to evict for a Register",
    ))
}

/// A connection once the frontend holds none, within [`TRIES`]: a dial refused past the bound of one is made again.
async fn dial_once_free(stage: &Stage) -> TestResult<Client> {
    for _ in 0..TRIES {
        match stage.ann().await {
            Ok(client) => return Ok(client),
            Err(_) => pause(2).await,
        }
    }
    Err(TestError::new(
        "the frontend did not end the closed connection",
    ))
}

/// Commits through `session` once its commit in flight has ended, within [`TRIES`]: a Commit refused with code 14 is sent again.
async fn commit_once_free(client: &mut Client, stage: &Stage, session: [u8; 16]) -> TestResult {
    for _ in 0..TRIES {
        match refused_through(client, stage, session).await? {
            None => return Ok(()),
            Some(ErrorCode::SessionBusy) => pause(2).await,
            Some(code) => {
                return Err(TestError::new(format!(
                    "the Commit was refused with {code:?}"
                )));
            }
        }
    }
    Err(TestError::new("the session's commit in flight did not end"))
}

/// Sends a Commit through `session` as its request 1, on a transaction of its own writing to partition 1: the transaction's
/// stream.
async fn commit_through(client: &mut Client, stage: &Stage, session: [u8; 16]) -> TestResult<u64> {
    let transaction = client.begin(0).await?;
    client.write(transaction, &stage.on(1)?).await?;
    client.on(transaction, &Commit::new(session, 1, 0)).await?;
    Ok(transaction)
}

/// The code a Commit through `session` is answered with, when its partition answers no Commit.
async fn refused_through(
    client: &mut Client,
    stage: &Stage,
    session: [u8; 16],
) -> TestResult<Option<ErrorCode>> {
    let transaction = commit_through(client, stage, session).await?;
    Ok(client.got(transaction).await?.code)
}

/// Answers the Commit `partition` holds through `session`.
fn answer_commit(partition: &RoleDouble, session: [u8; 16]) -> TestResult {
    let commits = partition.frames(Command::Commit);
    let held = commits
        .iter()
        .find(|frame| {
            view_frame(frame.bytes())
                .and_then(|view| Ok(CommitView::from_frame(&view)?.session()))
                .is_ok_and(|through| through == session)
        })
        .context("the Commit held through the session")?;
    held.answer_with(last_frame(
        CLUSTER,
        held.stream(),
        &Commit::new(session, 1, BIND_VERSION),
    )?);
    Ok(())
}

#[when(
    "the registry holds `sessions_max` sessions, the oldest has a Commit at its partition unanswered, its client's connection closes, and another session is registered"
)]
fn a_session_with_a_commit_in_flight_is_not_evicted(w: &mut Steps) -> TestResult {
    // One connection at a time: the second is let in only once the first's has ended, its session dropped with it.
    let options = Options {
        sessions: 2,
        connections: 1,
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let answering = Rc::new(Cell::new(false));
        let partition = holding_commits(&stage, Rc::clone(&answering)).await?;
        let mut first = stage.ann().await?;
        let (oldest, next) = (first.register().await?, first.register().await?);
        let _held = commit_through(&mut first, &stage, oldest).await?;
        until("the Commit", || partition.count(Command::Commit) == 1).await?;
        drop(first);
        let mut second = dial_once_free(&stage).await?;
        let _third = second.register().await?;
        let evicted = refused_through(&mut second, &stage, next).await?;
        assert_eq!(evicted, Some(ErrorCode::SessionEvicted), "the next oldest");
        let busy = refused_through(&mut second, &stage, oldest).await?;
        assert_eq!(
            busy,
            Some(ErrorCode::SessionBusy),
            "held, its commit in flight"
        );
        answer_commit(&partition, oldest)?;
        // The answer reaches a role connection whose client is gone: a Commit through the session is refused as busy until it
        // has, and once it is answered the session is free, the least recently registered.
        answering.set(true);
        commit_once_free(&mut second, &stage, oldest).await?;
        let _fourth = second.register().await?;
        let gone = refused_through(&mut second, &stage, oldest).await?;
        assert_eq!(
            gone,
            Some(ErrorCode::SessionEvicted),
            "the oldest, once its Commit was answered"
        );
        Ok(())
    })
}

#[then(
    "the registry evicts the next oldest session instead, and the oldest only once its partition has answered the Commit"
)]
fn a_session_with_a_commit_in_flight_is_not_evicted_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "the registry holds `sessions_max` sessions, each with a commit in flight, and a client registers another"
)]
fn a_full_registry_of_commits_in_flight_refuses_a_register_as_busy(w: &mut Steps) -> TestResult {
    let options = Options {
        sessions: 2,
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let partition = holding_commits(&stage, Rc::new(Cell::new(false))).await?;
        let mut client = stage.ann().await?;
        let sessions = [client.register().await?, client.register().await?];
        let mut transactions = Vec::new();
        for session in sessions {
            transactions.push(commit_through(&mut client, &stage, session).await?);
        }
        until("both Commits", || partition.count(Command::Commit) == 2).await?;
        let register = client.send(&Register::new([0; 16])).await?;
        let (code, _) = client.refused(register).await?;
        assert_eq!(code, ErrorCode::SessionBusy);
        for session in sessions {
            let held = refused_through(&mut client, &stage, session).await?;
            assert_eq!(held, Some(ErrorCode::SessionBusy), "nothing is evicted");
        }
        answer_commit(&partition, sessions[0])?;
        let answered = client
            .got(*transactions.first().context("the first transaction")?)
            .await?;
        assert_eq!(answered.command, Command::Commit, "{}", answered.message);
        let _registered = client.register().await.map_err(|error| {
            TestError::new(format!("the Register after a commit's answer: {error}"))
        })?;
        Ok(())
    })
}

#[then(
    "the Register is refused with code 14, nothing is evicted, and a Register after a commit is answered is served"
)]
fn a_full_registry_of_commits_in_flight_refuses_a_register_as_busy_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

/// Answers the DirectWrite `partition` holds through `session`, as its commit at [`BIND_VERSION`].
fn answer_write(partition: &RoleDouble, session: [u8; 16]) -> TestResult {
    let writes = partition.frames(Command::DirectWrite);
    let held = writes.first().context("the DirectWrite held")?;
    held.answer_with(last_frame(
        CLUSTER,
        held.stream(),
        &Commit::new(session, 1, BIND_VERSION),
    )?);
    Ok(())
}

#[when(
    "the registry holds `sessions_max` sessions, one with a Commit and one with a one-frame write at its partition unanswered, their client cancels both streams, and registers another session"
)]
fn a_cancel_after_a_commit_reaches_its_partition_holds_its_session(w: &mut Steps) -> TestResult {
    let options = Options {
        sessions: 2,
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let partition = holding_commits(&stage, Rc::new(Cell::new(false))).await?;
        let mut client = stage.ann().await?;
        let (committing, writing) = (client.register().await?, client.register().await?);
        let transaction = commit_through(&mut client, &stage, committing).await?;
        let mut terms = TermTable::new();
        let change = insert_into(&mut terms, &stage.on(1)?);
        let write = client
            .send(&DirectWrite::new(0, terms, vec![change]).with_session(writing, 1))
            .await?;
        until("the Commit and the DirectWrite", || {
            partition.count(Command::Commit) == 1 && partition.count(Command::DirectWrite) == 1
        })
        .await?;
        client.on(transaction, &Cancel).await?;
        client.on(write, &Cancel).await?;
        // The Cancels and the Register reach the owner in the order the connection sent them.
        let register = client.send(&Register::new([0; 16])).await?;
        let (code, _) = client.refused(register).await?;
        assert_eq!(code, ErrorCode::SessionBusy, "neither session evicted");
        answer_commit(&partition, committing)?;
        answer_write(&partition, writing)?;
        let _registered = register_once_free(&mut client).await?;
        Ok(())
    })
}

#[then(
    "the Register is refused with code 14, evicting neither, and a Register once the partition has answered both is served"
)]
fn a_cancel_after_a_commit_reaches_its_partition_holds_its_session_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}
