/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The steps of a read's level, token, deadline, timeout, refusal and retry.

use std::time::Duration;

use cucumber::{then, when};

use ultragraphdb_client_core::{
    BatchPoll, ClientConfig, ClientError, ClientErrorEnum, RunsAgain, TestServer,
};
use ultragraphdb_core_ids::{Epoch, SessionToken};
use ultragraphdb_protocol_transport::{Anonymous, NetworkFaults};
use ultragraphdb_protocol_wire::{
    Begin, BodyBuilder, Command, ErrorCode, FLAG_LAST, Header, ResultBatch, Welcome,
};
use ultragraphdb_tool_bdd::{Context as _, TestResult};

use crate::in_doubt_steps::{count, frames};
use crate::utils::harness::{CLUSTER, Harness, RELEASE, begun, config, server};
use crate::{Steps, begin, read, session};

/// A token of `epoch`, as an earlier commit's reply gives it.
fn token_of(epoch: u64) -> TestResult<SessionToken> {
    Ok(SessionToken::new(1, 5, Epoch::new(epoch)?))
}

/// A read-only Begin at the default level carrying a token of `epoch`, with `deadline_ms`.
fn begin_with_token(epoch: u64, deadline_ms: u32) -> TestResult<Begin> {
    Ok(Begin::new(0, 0, true)
        .with_deadline_ms(deadline_ms)
        .with_token(token_of(epoch)?))
}

/// Every batch of `stream` to its end, each read's epoch; the error that ends it, if one does.
fn epochs(
    h: &mut Harness,
    stream: &ultragraphdb_client_core::ResultStream,
) -> TestResult<(Vec<u64>, Option<ClientError>)> {
    let mut read = Vec::new();
    let mut failed = None;
    h.until(|h| {
        loop {
            match h.core.next_batch(stream, h.now, |batch| batch.epoch()) {
                Ok(BatchPoll::Batch(epoch)) => read.push(epoch),
                Ok(BatchPoll::Done) => return true,
                Ok(BatchPoll::Wait) => return false,
                Err(error) => {
                    failed = Some(error);
                    return true;
                }
            }
        }
    })?;
    Ok((read, failed))
}

#[when(
    "a read-only transaction begins with a token of epoch 30 while epoch 29 is the latest closed, reads, and epoch 30 closes"
)]
fn future_token(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server().with_closed_epoch(29))?;
    let p = h.core.begin_with(begin_with_token(30, 0)?, None, h.now)?;
    let tx = begun(h.settle(p)?)?;
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    while h.step()? {}
    assert!(
        matches!(
            h.core.next_batch(&stream, h.now, |_| ()),
            Ok(BatchPoll::Wait)
        ),
        "no answer before the close"
    );
    h.server.close_epoch(30);
    let (read, failed) = epochs(&mut h, &stream)?;
    assert_eq!(failed, None);
    w.numbers = read;
    Ok(())
}

#[then("the read answers after epoch 30 closes, at that epoch or later")]
fn answered_after_the_close(w: &mut Steps) -> TestResult {
    assert!(!w.numbers.is_empty(), "it answered");
    assert!(
        w.numbers.iter().all(|epoch| *epoch >= 30),
        "{:?}",
        w.numbers
    );
    Ok(())
}

#[when(
    "a read-only transaction begins with a token of epoch 30 and a deadline of 50 ms, and epoch 30 does not close within it"
)]
fn short_deadline(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server().with_closed_epoch(29))?;
    let p = h.core.begin_with(begin_with_token(30, 50)?, None, h.now)?;
    let tx = begun(h.settle(p)?)?;
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    let (read, failed) = epochs(&mut h, &stream)?;
    assert!(read.is_empty(), "no row");
    w.errors.push(failed.context("a refusal")?);
    w.harness = Some(h);
    Ok(())
}

#[then("its read is refused as timed out")]
fn timed_out(w: &mut Steps) -> TestResult {
    let answer = w.errors.first().context("the refusal")?;
    assert!(
        matches!(
            answer.0,
            ClientErrorEnum::Refused {
                code: ErrorCode::TimedOut,
                ..
            }
        ),
        "{answer:?}"
    );
    let h = w.harness.as_ref().context("a harness")?;
    assert!(h.core.is_ready(), "the refusal is the read's alone");
    Ok(())
}

#[when("a Result frame arrives for a read the client has cancelled")]
fn late_result(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    let tx = begin(&mut h, true)?;
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    while h.step()? {}
    let id = stream.stream();
    h.core.cancel(stream);
    while h.step()? {}
    let frame = ResultBatch::new(2).frame(Header::new(CLUSTER, Command::Result).with_stream(id))?;
    let frame2 = h.numbered(&frame)?;
    w.outcomes.push(h.deliver(&frame2)?);
    w.harness = Some(h);
    Ok(())
}

#[then("the client checks the frame and drops it, and the connection stays open")]
fn late_dropped(w: &mut Steps) -> TestResult {
    assert_eq!(w.outcomes.first(), Some(&Ok(())));
    let h = w.harness.as_ref().context("a harness")?;
    assert!(h.core.is_ready());
    Ok(())
}

#[when("a frame arrives on a stream at or above the next stream the client will open")]
fn frame_on_a_stream_not_opened(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    let tx = begin(&mut h, true)?;
    // The transaction took stream 1 and the read 2, so 3 is the next stream the client will open.
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    while h.step()? {}
    let next = stream.stream() + 1;
    let frame = ResultBatch::new(2).frame(
        Header::new(CLUSTER, Command::Result)
            .with_stream(next)
            .with_flags(FLAG_LAST),
    )?;
    let frame2 = h.numbered(&frame)?;
    w.outcomes.push(h.deliver(&frame2)?);
    w.harness = Some(h);
    Ok(())
}

#[then("the client refuses it, and closes the connection")]
fn refused_and_closed(w: &mut Steps) -> TestResult {
    let outcome = w.outcomes.first().context("the outcome")?;
    let refusal = outcome.as_ref().err().context("a refusal")?;
    assert_eq!(
        refusal,
        &ClientError::protocol(
            ultragraphdb_error_protocol::ProtocolError::command_not_served(
                Command::Result.number()
            )
        )
    );
    let h = w.harness.as_ref().context("a harness")?;
    assert!(!h.core.is_connected());
    Ok(())
}

#[when("a read delivers two batches and is then refused with code 24")]
fn refused_after_rows(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    h.server.refuse_next_read_after(2, ErrorCode::TimedOut, 1);
    let tx = begin(&mut h, true)?;
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    let (read, failed) = epochs(&mut h, &stream)?;
    w.numbers = vec![read.len() as u64, stream.stream()];
    w.errors.push(failed.context("a refusal")?);
    w.harness = Some(h);
    w.transaction = Some(tx);
    Ok(())
}

#[then(
    "the client returns the two batches and then the refusal, sends no Credit or Cancel on the stream, and the transaction stays open"
)]
fn batches_then_refusal(w: &mut Steps) -> TestResult {
    assert_eq!(w.numbers.first(), Some(&2));
    let answer = w.errors.first().context("the refusal")?;
    assert!(
        matches!(
            answer.0,
            ClientErrorEnum::Refused {
                code: ErrorCode::TimedOut,
                ..
            }
        ),
        "{answer:?}"
    );
    let stream = *w.numbers.get(1).context("the stream")?;
    let h = w.harness.as_mut().context("a harness")?;
    let quiet = frames(h)
        .iter()
        .filter(|(command, on)| {
            *on == stream && [Command::Credit.number(), Command::Cancel.number()].contains(command)
        })
        .count();
    assert_eq!(quiet, 0);
    let tx = w.transaction.context("a transaction")?;
    let again = h.core.read(&tx, &read(&tx), h.now)?;
    assert!(
        again.stream() > stream,
        "the transaction takes another read"
    );
    Ok(())
}

/// The refusal of a read of `code`.
fn read_refused(h: &mut Harness, code: ErrorCode) -> TestResult<ClientError> {
    h.server.refuse_next_read_after(0, code, 0);
    let tx = begin(h, true)?;
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    let (_, failed) = epochs(h, &stream)?;
    failed.context("a refusal")
}

/// The answer of a Commit refused with `code`.
fn commit_refused(h: &mut Harness, code: ErrorCode) -> TestResult<ClientError> {
    let session = session(h)?;
    h.server.refuse_next_commit(code, 0);
    let tx = begin(h, false)?;
    let p = h.core.commit(tx, session, h.now)?;
    h.settle(p)?.err().context("an error")
}

#[when("a commit is refused with code 2, and a read with code 24")]
fn conflict_and_timeout(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    w.errors.push(commit_refused(&mut h, ErrorCode::Conflict)?);
    w.errors.push(read_refused(&mut h, ErrorCode::TimedOut)?);
    Ok(())
}

#[then(
    "the client's error says of the first that the whole transaction runs again on a new Begin, and of the second that the read runs again"
)]
fn says_what_runs_again(w: &mut Steps) -> TestResult {
    let answers: Vec<_> = w.errors.iter().map(ClientError::runs_again).collect();
    assert_eq!(
        answers,
        vec![Some(RunsAgain::Transaction), Some(RunsAgain::Read)]
    );
    Ok(())
}

#[when("a read is refused with code 17, and a Commit with code 17")]
fn unavailable_twice(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    w.errors.push(read_refused(&mut h, ErrorCode::Unavailable)?);
    w.errors
        .push(commit_refused(&mut h, ErrorCode::Unavailable)?);
    Ok(())
}

#[then(
    "the client's error says of the first that the read runs again, and of the second that it is in doubt"
)]
fn read_again_commit_in_doubt(w: &mut Steps) -> TestResult {
    let answers: Vec<_> = w.errors.iter().map(ClientError::runs_again).collect();
    assert_eq!(
        answers,
        vec![Some(RunsAgain::Read), Some(RunsAgain::InDoubt)]
    );
    Ok(())
}

#[when(
    "a read-only transaction's deadline is 40 s, and its read waits 35 s for a close, the client's timeout set at 30 s"
)]
fn long_deadline(w: &mut Steps) -> TestResult {
    let mut h = Harness::with(
        config().with_timeout(Duration::from_secs(30)),
        server().with_closed_epoch(29),
        NetworkFaults::none(),
        1,
        Anonymous,
    );
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    let p = h
        .core
        .begin_with(begin_with_token(30, 40_000)?, None, h.now)?;
    let tx = begun(h.settle(p)?)?;
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    while h.step()? {}
    h.now += Duration::from_secs(35);
    h.core.handle_timeout(h.now);
    h.server.handle_timeout(h.now);
    assert!(h.core.is_ready(), "the wait is within the deadline");
    assert_eq!(h.connections, 1);
    h.server.close_epoch(30);
    let (read, failed) = epochs(&mut h, &stream)?;
    assert_eq!(failed, None);
    w.numbers = read;
    w.harness = Some(h);
    Ok(())
}

#[then("the connection stays open, and the read answers once the close arrives")]
fn survived(w: &mut Steps) -> TestResult {
    assert!(!w.numbers.is_empty());
    let h = w.harness.as_ref().context("a harness")?;
    assert!(h.core.is_ready());
    assert_eq!(h.connections, 1);
    Ok(())
}

#[when(
    "a read's stream has spent its credit, and its caller takes no batch for longer than the timeout"
)]
fn no_credit(w: &mut Steps) -> TestResult {
    let welcome = Welcome::new(1, RELEASE, "s")
        .with_max_frame(1 << 20)
        .with_max_streams(8)
        .with_initial_credit(1);
    let mut h = Harness::with(
        ClientConfig::clone(&config()),
        TestServer::new(CLUSTER, welcome).with_results(4),
        NetworkFaults::none(),
        1,
        Anonymous,
    );
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    let tx = begin(&mut h, true)?;
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    while h.step()? {}
    // The read past the first frame: its inbox empty and its credit spent, what it waits on is its caller.
    assert!(matches!(
        h.core.next_batch(&stream, h.now, |_| ()),
        Ok(BatchPoll::Batch(()))
    ));
    assert_eq!(count(&h, Command::Credit), 0);
    assert_eq!(h.core.next_deadline(), None, "nothing is awaited");
    h.now += h.core.config().timeout() * 5;
    h.core.handle_timeout(h.now);
    w.harness = Some(h);
    Ok(())
}

#[then("the connection stays open")]
fn stays_open(w: &mut Steps) -> TestResult {
    let h = w.harness.as_ref().context("a harness")?;
    assert!(h.core.is_ready());
    assert_eq!(h.connections, 1);
    Ok(())
}

#[when(
    "a client commits a write, receives a session token, and begins a transaction with that token whose read asks for the write"
)]
fn read_after_commit(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    let session = session(&mut h)?;
    h.server.set_epoch(12);
    let p = h
        .core
        .direct_write(crate::in_doubt_steps::insert(), session, h.now)?;
    let reply = crate::utils::harness::committed(h.settle(p)?)?;
    let p2 = h.core.begin_with(Begin::new(0, 0, true), session, h.now)?;
    let tx = begun(h.settle(p2)?)?;
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    let (read, failed) = epochs(&mut h, &stream)?;
    assert_eq!(failed, None);
    w.numbers = read;
    w.answers.push(reply);
    Ok(())
}

#[then("the read returns the write")]
fn read_returns_the_write(w: &mut Steps) -> TestResult {
    let token = w
        .answers
        .first()
        .and_then(|a| a.token())
        .context("a token")?;
    assert!(!w.numbers.is_empty());
    assert!(
        w.numbers.iter().all(|epoch| *epoch >= token.epoch().get()),
        "{:?}",
        w.numbers
    );
    Ok(())
}
