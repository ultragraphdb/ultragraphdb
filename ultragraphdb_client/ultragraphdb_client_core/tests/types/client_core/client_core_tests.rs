/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use ultragraphdb_client_core::{ClientCore, ClientError, ClientErrorEnum, TestServer, Transaction};
use ultragraphdb_error_protocol::{ProtocolError, ProtocolErrorEnum};
use ultragraphdb_protocol_transport::{Anonymous, BufferPool, NetworkFaults, TransportError};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, ColumnType, Command, CommitOperation, ErrorCode, ErrorReply, FrameView,
    Header, HeaderView, HelloView, MAX_FRAME_SIZE, Operation, PatternSlot, Prepare, QuadPattern,
    TermTable, Welcome, Write,
};

use crate::utils::harness::{
    CLUSTER, Harness, RELEASE, UNIX, begun, config, described, prepared, received, registered,
    server, welcome,
};
use ultragraphdb_tool_bdd::{Context as _, TestResult};

pub fn counter(terms: &mut TermTable) -> Operation {
    let (subject, predicate) = (terms.iri("http://ex.org/c"), terms.iri("http://ex.org/n"));
    Operation::new(
        terms.clone(),
        CommitOperation::CounterAdd {
            graph: None,
            subject,
            predicate,
            delta: 1,
        },
    )
}

#[test]
fn test_connect_sends_the_hello_and_a_welcome_makes_it_ready() -> TestResult {
    let mut core = ClientCore::new(config(), Anonymous);
    assert!(!core.is_ready());
    assert_eq!(core.transmit(), None);
    core.connect(crate::utils::harness::start(), UNIX)?;
    let hello = core.transmit().context("a frame to transmit")?;
    assert_eq!(core.transmit(), None, "the Hello alone");
    // PROTOCOL.md section 11.1, step 1: the first frame, on stream 0, at protocol 1, of the cluster and release.
    let header = HeaderView::new(&hello)?;
    assert_eq!(
        (
            header.sequence(),
            header.stream(),
            header.protocol(),
            header.cluster(),
            header.release()
        ),
        (1, 0, 1, CLUSTER, RELEASE)
    );
    let view = FrameView::validate(&hello, CLUSTER, MAX_FRAME_SIZE)?;
    let body = HelloView::from_frame(&view)?;
    // The configuration's: protocols 1 to the crate's, 1, the largest frame, 1 MiB, and its name.
    assert_eq!(
        (
            body.protocol_min(),
            body.protocol_max(),
            body.release(),
            body.max_frame(),
            body.client()
        ),
        (1, 1, RELEASE, 1_048_576, "ultragraph-test")
    );
    let h = Harness::ready(server())?;
    assert_eq!(h.core.welcome(), Some(&welcome()));
    assert_eq!(h.core.next_deadline(), None, "nothing is awaited");
    Ok(())
}

#[test]
fn test_a_welcome_outside_the_range_closes_the_connection() -> TestResult {
    let mut core = ClientCore::new(config(), Anonymous);
    let now = crate::utils::harness::start();
    core.connect(now, UNIX)?;
    let _hello: Option<Vec<u8>> = core.transmit();
    let pool = BufferPool::new(2, 1 << 20);
    let other = Welcome::new(2, 0, "s")
        .with_max_frame(1 << 20)
        .with_max_streams(1)
        .with_initial_credit(1)
        .frame(Header::new(CLUSTER, Command::Welcome).with_sequence(1))?;
    let refused = core
        .receive(received(&pool, &other)?, now)
        .err()
        .context("an error")?;
    // Section 11.1, step 5: a protocol outside the Hello's range, 1 to 1, refused naming the Welcome's 2.
    assert_eq!(
        refused.0,
        ClientErrorEnum::Transport {
            error: TransportError::protocol(ProtocolError::field_value(2, "protocol", 2))
        }
    );
    assert!(!core.is_ready());
    assert!(!core.is_connected(), "the connection is given up");
    Ok(())
}

#[test]
fn test_requests_before_the_welcome_are_refused_as_not_connected() -> TestResult {
    let mut core = ClientCore::new(config(), Anonymous);
    let now = crate::utils::harness::start();
    assert_eq!(
        core.register(now).err().context("an error")?.0,
        ClientErrorEnum::NotConnected
    );
    core.connect(now, UNIX)?;
    assert_eq!(
        core.begin(0, false, now).err().context("an error")?.0,
        ClientErrorEnum::NotConnected
    );
    Ok(())
}

/// A Register waiting to be sent again finds the connection's one stream taken: what stopped its resend is its answer.
#[test]
fn test_a_register_that_cannot_be_sent_again_answers_why() -> TestResult {
    let streams = 1;
    let mut scripted = TestServer::new(CLUSTER, welcome().with_max_streams(streams));
    scripted.answer_busy_registers(1);
    let mut h = Harness::ready(scripted)?;
    let p = h.core.register(h.now)?;
    while h.step()? {}
    assert!(h.core.take_reply(p).is_none(), "it waits to be sent again");
    let _taken = h.core.begin(0, true, h.now)?;
    let answer = h.settle(p)?.err().context("its answer")?;
    assert_eq!(answer.0, ClientErrorEnum::StreamLimit { limit: streams });
    Ok(())
}

#[test]
fn test_register_and_begin_answer_a_session_and_a_snapshot() -> TestResult {
    let mut h = Harness::ready(server())?;
    let p = h.core.register(h.now)?;
    let session = registered(h.settle(p)?)?;
    assert_eq!(session.number(), 9);
    assert_eq!(h.core.next_request(session), Some(1));
    let beginning = h.core.begin(3, true, h.now)?;
    let tx = begun(h.settle(beginning)?)?;
    assert_eq!(
        (tx.tenant(), tx.read_version(), tx.read_only()),
        (3, h.server.version(), true)
    );
    assert_eq!(
        h.core.take_reply(beginning),
        None,
        "an answer is taken once"
    );
    Ok(())
}

#[test]
fn test_a_read_only_transaction_neither_writes_nor_commits() -> TestResult {
    let mut h = Harness::ready(server())?;
    let p = h.core.register(h.now)?;
    let session = registered(h.settle(p)?)?;
    let beginning = h.core.begin(0, true, h.now)?;
    let tx = begun(h.settle(beginning)?)?;
    let stream = tx.stream();
    let write = Write::new(TermTable::new(), Vec::new());
    assert_eq!(
        h.core.write(&tx, &write).err().context("an error")?.0,
        ClientErrorEnum::ReadOnly { stream }
    );
    assert_eq!(
        h.core
            .operation(&tx, &counter(&mut TermTable::new()))
            .err()
            .context("an error")?
            .0,
        ClientErrorEnum::ReadOnly { stream }
    );
    assert_eq!(
        h.core
            .commit(tx, session, h.now)
            .err()
            .context("an error")?
            .0,
        ClientErrorEnum::ReadOnly { stream }
    );
    h.core.abort(tx)?;
    h.until(|h| h.server.received().len() >= 4)?;
    assert_eq!(
        HeaderView::new(h.server.received().last().context("a frame received")?)?.command(),
        Command::Abort.number()
    );
    assert_eq!(
        h.core.write(&tx, &write).err().context("an error")?.0,
        ClientErrorEnum::StreamFinished { stream },
        "aborted"
    );
    Ok(())
}

#[test]
fn test_prepare_describe_and_a_stale_statement() -> TestResult {
    let mut h = Harness::ready(server())?;
    let mut terms = TermTable::new();
    let name = terms.iri("http://ex.org/name");
    let pattern = QuadPattern::new(
        PatternSlot::DefaultGraph,
        PatternSlot::Parameter(0),
        PatternSlot::Bound(name),
        PatternSlot::Unbound,
    );
    let prepare = Prepare::new(0, 0, 0, terms, pattern, vec![ColumnType::Node], Vec::new());
    let p = h.core.prepare(&prepare, h.now)?;
    let statement = prepared(h.settle(p)?)?;
    assert_eq!(
        (statement.id(), statement.schema(), statement.tenant()),
        (1, 1, 0)
    );
    assert_eq!(statement.parameter_types(), &[ColumnType::Node]);
    assert!(statement.is_current(1) && !statement.is_current(2));
    let describing = h.core.describe(0, h.now)?;
    let describe = described(h.settle(describing)?)?;
    assert_eq!(describe.read_version(), h.server.version());
    Ok(())
}

#[test]
fn test_a_malformed_reply_closes_the_connection_and_loses_its_requests() -> TestResult {
    let mut h = Harness::ready(server())?;
    let p = h.core.describe(0, h.now)?;
    while h.core.transmit().is_some() {}
    let mut frame = ultragraphdb_protocol_wire::Describe::new(0, 5)
        .frame(Header::new(CLUSTER, Command::Describe).with_stream(1))?;
    let last = frame.len() - 1;
    frame[last] ^= 4;
    let frame = h.numbered(&frame)?;
    let refused = h.deliver(&frame)?;
    let error = refused.err().context("refused")?;
    assert!(
        matches!(error.0, ClientErrorEnum::Protocol { error: ref e } if matches!(e.0, ProtocolErrorEnum::Chunk { index: 0, .. })),
        "{error:?}"
    );
    assert!(!h.core.is_ready());
    assert_eq!(
        h.core
            .take_reply(p)
            .context("an answer")?
            .err()
            .context("an error")?,
        ClientError::connection_lost(Some(error)),
        "the rule is the loss's cause"
    );
    Ok(())
}

#[test]
fn test_an_error_on_stream_zero_closes_the_connection() -> TestResult {
    let mut h = Harness::ready(server())?;
    let frame = ErrorReply::new(ErrorCode::Protocol, "bad frame")
        .frame(Header::new(CLUSTER, Command::Error))?;
    let frame = h.numbered(&frame)?;
    let error = h.deliver(&frame)?.err().context("an error")?;
    assert_eq!(
        error.0,
        ClientErrorEnum::Closed {
            code: ErrorCode::Protocol,
            message: "bad frame".into()
        }
    );
    assert!(!h.core.is_ready());
    Ok(())
}

#[test]
fn test_an_unanswered_request_times_out_and_closes_the_connection() -> TestResult {
    let mut h = Harness::ready(server())?;
    h.blackhole = true;
    let p = h.core.register(h.now)?;
    while h.step()? {}
    let deadline = h.core.next_deadline().context("a deadline")?;
    assert_eq!(deadline, h.now + Duration::from_secs(1));
    h.core.handle_timeout(deadline - Duration::from_millis(1));
    assert_eq!(h.core.take_reply(p), None, "not yet");
    h.core.handle_timeout(deadline);
    assert_eq!(
        h.core
            .take_reply(p)
            .context("an answer")?
            .err()
            .context("an error")?
            .0,
        ClientErrorEnum::TimedOut
    );
    assert!(!h.core.is_ready());
    assert_eq!(h.core.next_deadline(), None);
    Ok(())
}

#[test]
fn test_streams_beyond_the_welcomes_limit_are_refused() -> TestResult {
    let mut h = Harness::with(
        config(),
        TestServer::new(
            CLUSTER,
            Welcome::new(1, RELEASE, "s")
                .with_max_frame(1 << 20)
                .with_max_streams(1)
                .with_initial_credit(4),
        ),
        NetworkFaults::none(),
        1,
        Anonymous,
    );
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    let p = h.core.begin(0, false, h.now)?;
    assert_eq!(
        h.core.begin(0, false, h.now).err().context("an error")?.0,
        ClientErrorEnum::StreamLimit { limit: 1 }
    );
    let _tx: Transaction = begun(h.settle(p)?)?;
    Ok(())
}

#[test]
fn test_a_request_past_the_welcomes_frame_is_refused_before_it_is_sent() -> TestResult {
    let mut h = Harness::with(
        config(),
        TestServer::new(
            CLUSTER,
            Welcome::new(1, RELEASE, "s")
                .with_max_frame(524_288)
                .with_max_streams(8)
                .with_initial_credit(4),
        ),
        NetworkFaults::none(),
        1,
        Anonymous,
    );
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    let p = h.core.begin(0, false, h.now)?;
    let tx = begun(h.settle(p)?)?;
    let mut terms = TermTable::new();
    let big = "x".repeat(530_000);
    let _big: u32 = terms.iri(&big);
    let error = h
        .core
        .write(&tx, &Write::new(terms, Vec::new()))
        .err()
        .context("an error")?;
    assert!(
        matches!(error.0, ClientErrorEnum::Request { error: ref e } if matches!(e.0, ProtocolErrorEnum::FrameTooLarge { limit: 524_288, .. })),
        "{error:?}"
    );
    Ok(())
}

/// Frames are numbered: a reply repeated is out of sequence, and the connection is given up after its first copy
/// was taken.
#[test]
fn test_a_repeated_reply_is_refused_after_its_first_is_taken() -> TestResult {
    let mut h = Harness::ready(server())?;
    let p = h.core.register(h.now)?;
    while h.core.transmit().is_some() {}
    let frame = ultragraphdb_protocol_wire::Register::new(77u128.to_be_bytes())
        .frame(Header::new(CLUSTER, Command::Register).with_stream(1))?;
    let frame = h.numbered(&frame)?;
    h.deliver(&frame)??;
    let refused = h.deliver(&frame)?.err().context("an error")?;
    assert!(
        matches!(&refused.0, ClientErrorEnum::Protocol { error } if matches!(error.0, ProtocolErrorEnum::SequenceOrder { .. })),
        "{refused:?}"
    );
    let session = registered(h.core.take_reply(p).context("an answer")?)?;
    assert_eq!(session.number(), 77);
    assert!(!h.core.is_ready());
    assert_eq!(
        FrameView::validate(&frame, CLUSTER, MAX_FRAME_SIZE)?.command(),
        Command::Register
    );
    Ok(())
}

/// A commit reply naming another request than the one sent is no answer of it, which stays in doubt.
#[test]
fn test_a_commit_reply_of_another_request_is_a_mismatch() -> TestResult {
    let mut h = Harness::ready(server())?;
    let p = h.core.register(h.now)?;
    let session = registered(h.settle(p)?)?;
    let beginning = h.core.begin(0, false, h.now)?;
    let tx = begun(h.settle(beginning)?)?;
    let committing = h.core.commit(tx, session, h.now)?;
    while h.core.transmit().is_some() {}
    {
        let (field, reply) = (
            "request",
            ultragraphdb_protocol_wire::Commit::new(
                session.number().to_be_bytes(),
                2,
                h.server.version(),
            )
            .with_token(ultragraphdb_client_core::TestServer::token(
                h.server.version(),
            )),
        );
        let frame = reply.frame(Header::new(CLUSTER, Command::Commit).with_stream(tx.stream()))?;
        let frame = h.numbered(&frame)?;
        h.deliver(&frame)??;
        assert_eq!(
            h.core
                .take_reply(committing)
                .context("an answer")?
                .err()
                .context("an error")?
                .0,
            ClientErrorEnum::InDoubt {
                session: session.number(),
                request: 1,
                cause: Some(Box::new(ClientError::reply_mismatch(field, 1u64, 2u64))),
            }
        );
    }
    assert_eq!(
        h.core.next_request(session),
        Some(1),
        "no commit was answered"
    );
    Ok(())
}

/// A Write has no reply: its refusal ends the transaction, and is the answer of the transaction's next request.
#[test]
fn test_a_refused_write_is_the_answer_of_the_next_request() -> TestResult {
    let mut h = Harness::ready(server())?;
    let p = h.core.begin(0, false, h.now)?;
    let tx = begun(h.settle(p)?)?;
    h.core
        .write(&tx, &Write::new(TermTable::new(), Vec::new()))?;
    while h.core.transmit().is_some() {}
    let frame = ErrorReply::new(ErrorCode::Refused, "the catalog refuses").frame(
        Header::new(CLUSTER, Command::Error)
            .with_stream(tx.stream())
            .with_flags(ultragraphdb_protocol_wire::FLAG_LAST),
    )?;
    let frame = h.numbered(&frame)?;
    h.deliver(&frame)??;
    let refused = h
        .core
        .write(&tx, &Write::new(TermTable::new(), Vec::new()))
        .err()
        .context("an error")?;
    assert!(
        matches!(refused.0, ClientErrorEnum::Refused { code: ErrorCode::Refused, ref message, .. } if message == "the catalog refuses"),
        "{refused:?}"
    );
    assert_eq!(
        h.core
            .write(&tx, &Write::new(TermTable::new(), Vec::new()))
            .err()
            .context("an error")?
            .0,
        ClientErrorEnum::StreamFinished {
            stream: tx.stream()
        }
    );
    assert!(h.core.is_ready());
    Ok(())
}

/// A read at the server's mercy times out from the later of its progress and its deadline, by at least 30 s whatever the
/// client's timeout; a request's reply keeps the client's timeout.
#[test]
fn test_a_reads_timeout_runs_from_its_deadline_by_at_least_thirty_seconds() -> TestResult {
    use std::time::Instant;
    let floor = Duration::from_secs(30);
    let mut h = Harness::ready(server())?;
    h.blackhole = false;
    let p = h.core.begin_with(
        ultragraphdb_protocol_wire::Begin::new(0, 0, true).with_deadline_ms(40_000),
        None,
        h.now,
    )?;
    let tx = begun(h.settle(p)?)?;
    let begun_at: Instant = h.now;
    h.blackhole = true;
    let read = crate::types::client_core::client_core_results_tests::read(&tx);
    let stream = h.core.read(&tx, &read, h.now)?;
    while h.step()? {}
    assert_eq!(
        h.core.next_deadline(),
        Some(begun_at + Duration::from_secs(40) + floor),
        "the deadline, then the floor"
    );
    h.now = begun_at + Duration::from_secs(69);
    h.core.handle_timeout(h.now);
    assert!(h.core.is_ready());
    h.now = begun_at + Duration::from_secs(70);
    h.core.handle_timeout(h.now);
    assert!(!h.core.is_connected());
    assert_eq!(
        h.core.next_batch(&stream, h.now, |_| ()).err(),
        Some(ClientError::timed_out())
    );
    Ok(())
}

/// A deadline of 0 is the Welcome's `transaction_ms`, and a client's timeout above the floor is the client's.
#[test]
fn test_a_deadline_of_none_is_the_welcomes_and_a_longer_timeout_is_kept() -> TestResult {
    let transaction = Duration::from_millis(welcome().transaction_ms().into());
    let timeout = Duration::from_secs(50);
    let mut h = Harness::with(
        config().with_timeout(timeout),
        server(),
        NetworkFaults::none(),
        1,
        Anonymous,
    );
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    let p = h.core.begin(0, true, h.now)?;
    let tx = begun(h.settle(p)?)?;
    let at = h.now;
    h.blackhole = true;
    let read = crate::types::client_core::client_core_results_tests::read(&tx);
    let _stream = h.core.read(&tx, &read, h.now)?;
    while h.step()? {}
    assert_eq!(h.core.next_deadline(), Some(at + transaction + timeout));
    Ok(())
}

/// A read's progress moves its timeout: with none, it runs from the deadline; a frame arriving past the deadline and taken
/// restarts it from then, by the floor. The frame waiting to be taken runs none.
#[test]
fn test_a_reads_progress_moves_its_timeout() -> TestResult {
    let floor = Duration::from_secs(30);
    let one_frame = TestServer::new(CLUSTER, welcome().with_initial_credit(1)).with_results(3);
    let mut h = Harness::ready(one_frame)?;
    let begin = ultragraphdb_protocol_wire::Begin::new(0, 0, true).with_deadline_ms(10);
    let p = h.core.begin_with(begin.clone(), None, h.now)?;
    let tx = begun(h.settle(p)?)?;
    let deadline = h.now + Duration::from_millis(begin.deadline_ms().into());
    let read = crate::types::client_core::client_core_results_tests::read(&tx);
    let stream = h.core.read(&tx, &read, h.now)?;
    assert_eq!(
        h.core.next_deadline(),
        Some(deadline + floor),
        "no progress yet: from the deadline"
    );
    // The read reaches the server, and its one frame of credit arrives, 25 s on: past the deadline.
    h.now += Duration::from_secs(25);
    let arrived = h.now;
    while h.step()? {}
    assert_eq!(
        h.core.next_deadline(),
        None,
        "a frame to take runs no timeout"
    );
    assert_eq!(
        h.core.next_batch(&stream, h.now, |_| ())?,
        ultragraphdb_client_core::BatchPoll::Batch(())
    );
    assert_eq!(
        h.core.next_batch(&stream, h.now, |_| ())?,
        ultragraphdb_client_core::BatchPoll::Wait
    );
    assert_eq!(
        h.core.next_deadline(),
        Some(arrived + floor),
        "the frame's progress restarts it, past the deadline"
    );
    Ok(())
}

/// A timeout past what an `Instant` holds never fires, nor does a credential's expiry past it fall due: neither panics.
#[test]
fn test_a_timeout_or_an_expiry_past_an_instant_sets_no_deadline() -> TestResult {
    let mut h = Harness::with(
        config().with_timeout(Duration::MAX),
        server(),
        NetworkFaults::none(),
        1,
        Anonymous,
    );
    h.connect()?;
    assert_eq!(h.core.next_deadline(), None);
    h.until(|h| h.core.is_ready())?;
    let p = h.core.register(h.now)?;
    assert_eq!(h.core.next_deadline(), None);
    h.core.handle_timeout(h.now + Duration::from_secs(3600));
    assert!(h.core.take_reply(p).is_none());
    Ok(())
}

/// A Write of one IRI `iri_bytes` long.
fn write_of(iri_bytes: usize) -> Write {
    let mut terms = TermTable::new();
    let _iri: u32 = terms.iri(&"x".repeat(iri_bytes));
    Write::new(terms, Vec::new())
}

/// A request exactly as large as the Welcome's `max_frame` is sent: only a larger one is refused (`PROTOCOL.md` §2.5: a
/// frame up to `max_frame` bytes).
#[test]
fn test_a_request_of_the_welcomes_largest_frame_is_sent() -> TestResult {
    const MAX: usize = 524_288;
    let size = |iri_bytes: usize| -> TestResult<usize> {
        Ok(write_of(iri_bytes)
            .frame(Header::new(CLUSTER, Command::Write))?
            .len())
    };
    // A frame is its body and a chunk table growing with the body (`PROTOCOL.md` §2.3), so the IRI is sized again until
    // the frame comes out at the limit.
    let mut iri_bytes = MAX - size(0)?;
    let mut exact = None;
    for _ in 0..8 {
        let got = size(iri_bytes)?;
        if got == MAX {
            exact = Some(iri_bytes);
            break;
        }
        iri_bytes = (iri_bytes + MAX)
            .checked_sub(got)
            .context("a smaller IRI")?;
    }
    let exact = exact.context("an IRI filling the frame to its limit")?;
    let mut h = Harness::with(
        config(),
        TestServer::new(
            CLUSTER,
            Welcome::new(1, RELEASE, "s")
                .with_max_frame(524_288)
                .with_max_streams(8)
                .with_initial_credit(4),
        ),
        NetworkFaults::none(),
        1,
        Anonymous,
    );
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    let p = h.core.begin(0, false, h.now)?;
    let tx = begun(h.settle(p)?)?;
    h.core.write(&tx, &write_of(exact))?;
    assert_eq!(h.core.transmit().context("the Write")?.len(), MAX);
    Ok(())
}

/// An insert-if-absent of `http://ex.org/x`, its node created.
fn claim() -> ultragraphdb_protocol_wire::ConditionalWrite {
    let mut terms = TermTable::new();
    let predicate = terms.iri("http://ex.org/id");
    let value = terms.iri("http://ex.org/x");
    ultragraphdb_protocol_wire::ConditionalWrite::new(
        0,
        terms,
        Vec::new(),
        vec![CommitOperation::InsertIfAbsent {
            graph: None,
            predicate,
            value,
            properties: Vec::new(),
        }],
    )
}

/// The frames of `command` the server received.
fn received_of(h: &Harness, command: Command) -> usize {
    h.server
        .received()
        .iter()
        .filter(|f| HeaderView::new(f).is_ok_and(|header| header.command() == command.number()))
        .count()
}

/// A ConditionalWrite answered busy is sent again whole after the retry delay, and commits once.
#[test]
fn test_a_conditional_write_answered_busy_is_sent_again_whole() -> TestResult {
    let mut h = Harness::ready(server())?;
    let registering = h.core.register(h.now)?;
    let session = registered(h.settle(registering)?)?;
    h.server.refuse_next_commit(ErrorCode::SessionBusy, 0);
    let writing = h.core.conditional_write(claim(), session, h.now)?;
    let reply = h.settle(writing)?;
    let committed = crate::utils::harness::committed(reply)?;
    assert_eq!(committed.request(), 1);
    assert_eq!(received_of(&h, Command::ConditionalWrite), 2, "sent again");
    assert_eq!(h.server.applied(), 1, "applied once");
    Ok(())
}

/// A one-frame write lost with its connection, larger than the next connection's Welcome allows, cannot be sent again: it
/// was sent before and may have been applied, so it is in doubt, the frame's size its cause.
#[test]
fn test_a_write_too_large_to_be_sent_again_on_the_next_connection_is_in_doubt() -> TestResult {
    const SMALLER: u32 = 524_288;
    let mut h = Harness::ready(server())?;
    let registering = h.core.register(h.now)?;
    let session = registered(h.settle(registering)?)?;
    let mut terms = TermTable::new();
    let big = terms.iri(&"x".repeat(SMALLER as usize));
    let write = ultragraphdb_protocol_wire::DirectWrite::new(
        0,
        terms,
        vec![ultragraphdb_protocol_wire::WriteChange::Insert {
            graph: None,
            subject: big,
            predicate: big,
            object: big,
        }],
    );
    h.server.drop_after_next_commit();
    let writing = h.core.direct_write(write, session, h.now)?;
    h.until(|h| !h.core.is_connected())?;
    assert_eq!(
        h.server.applied(),
        1,
        "applied before the connection dropped"
    );
    // The next connection's server allows frames of half the size.
    h.server = TestServer::new(CLUSTER, welcome().with_max_frame(SMALLER));
    let error = h.settle(writing)?.err().context("an error")?;
    let ClientErrorEnum::InDoubt {
        session: s,
        request,
        cause,
    } = error.0
    else {
        return Err(ultragraphdb_tool_bdd::TestError::new(format!("{error:?}")));
    };
    let (in_doubt, cause) = ((s, request), cause);
    assert_eq!(in_doubt, (session.number(), 1));
    assert!(
        matches!(
            cause.as_deref().map(|c| &c.0),
            Some(ClientErrorEnum::Request { error: rule }) if matches!(
                rule.0,
                ProtocolErrorEnum::FrameTooLarge { limit, .. } if limit == SMALLER as usize
            )
        ),
        "{cause:?}"
    );
    assert!(h.core.is_ready(), "the new connection serves on");
    Ok(())
}

/// A commit whose frame had not left when its connection timed out is lost, not in doubt: its answer is the connection's
/// loss, the timeout its cause, and its session takes its next request.
#[test]
fn test_a_commit_not_yet_sent_when_its_connection_times_out_is_lost() -> TestResult {
    let mut h = Harness::ready(server())?;
    let registering = h.core.register(h.now)?;
    let session = registered(h.settle(registering)?)?;
    let beginning = h.core.begin(0, false, h.now)?;
    let tx = begun(h.settle(beginning)?)?;
    let committing = h.core.commit(tx, session, h.now)?;
    let due = h.core.next_deadline().context("the commit's timeout")?;
    h.core.handle_timeout(due);
    assert!(!h.core.is_connected());
    assert_eq!(
        h.core.take_reply(committing),
        Some(Err(ClientError::connection_lost(Some(
            ClientError::timed_out()
        ))))
    );
    assert_eq!(h.core.next_request(session), Some(1), "the number stays");
    Ok(())
}

/// The sessions the core holds, read from its `Debug`, which lists each with its counter.
fn sessions_held<L: ultragraphdb_protocol_transport::Login>(core: &ClientCore<L>) -> usize {
    format!("{core:?}").matches("next_request").count()
}

/// A request whose caller forgot it once it was sent is let go when its answer arrives: the core holds no more requests
/// than before, the session the forgotten Register was given is not kept, and a later Register is answered with its own
/// session. Forgetting a request already taken changes nothing.
#[test]
fn test_a_forgotten_request_is_let_go_when_its_answer_arrives() -> TestResult {
    let first = {
        let mut fresh = Harness::ready(server())?;
        let p = fresh.core.register(fresh.now)?;
        registered(fresh.settle(p)?)?
    };
    let mut h = Harness::ready(server())?;
    let held = h.core.pending_requests();
    let forgotten = h.core.register(h.now)?;
    h.core.forget(forgotten);
    assert_eq!(
        h.core.pending_requests(),
        held + 1,
        "it waits for its answer"
    );
    h.until(|h| h.core.pending_requests() == held)?;
    assert_eq!(h.core.take_reply(forgotten), None, "no answer is kept");
    assert_eq!(
        sessions_held(&h.core),
        0,
        "the forgotten session is not kept"
    );
    let p = h.core.register(h.now)?;
    let session = registered(h.settle(p)?)?;
    assert_ne!(session.number(), first.number(), "its own session");
    assert_eq!(h.core.next_request(session), Some(1));
    assert_eq!(sessions_held(&h.core), 1);
    h.core.forget(p);
    assert_eq!(h.core.pending_requests(), held);
    Ok(())
}

/// A forgotten Begin's transaction is aborted when its answer arrives, its stream let go on both sides; a later Begin
/// is answered with a transaction of its own.
#[test]
fn test_a_forgotten_begin_is_aborted_when_its_answer_arrives() -> TestResult {
    let mut h = Harness::ready(server())?;
    let held = h.core.pending_requests();
    let forgotten = h.core.begin(0, false, h.now)?;
    h.core.forget(forgotten);
    h.until(|h| h.core.pending_requests() == held)?;
    h.until(|h| h.server.open_transactions() == 0)?;
    assert!(
        format!("{:?}", h.core).contains("streams: 0"),
        "{:?}",
        h.core
    );
    let p = h.core.begin(0, false, h.now)?;
    let tx = begun(h.settle(p)?)?;
    assert!(!tx.read_only());
    assert_eq!(h.server.open_transactions(), 1);
    assert_eq!(h.core.pending_requests(), held);
    Ok(())
}

/// A request forgotten once its answer arrived is let go at once, a Begin's transaction aborted.
#[test]
fn test_a_request_forgotten_once_answered_is_let_go_at_once() -> TestResult {
    let mut h = Harness::ready(server())?;
    let held = h.core.pending_requests();
    let p = h.core.begin(0, false, h.now)?;
    while h.step()? {}
    assert_eq!(h.server.open_transactions(), 1);
    h.core.forget(p);
    assert_eq!(h.core.pending_requests(), held);
    while h.step()? {}
    assert_eq!(h.server.open_transactions(), 0, "aborted");
    Ok(())
}

/// A forgotten commit goes on to its answer, which moves its session's counter: the session's next commit takes the
/// next number and is answered.
#[test]
fn test_a_forgotten_commit_moves_its_sessions_counter() -> TestResult {
    let mut h = Harness::ready(server())?;
    let registering = h.core.register(h.now)?;
    let session = registered(h.settle(registering)?)?;
    let held = h.core.pending_requests();
    let first = h.core.next_request(session).context("a session")?;
    let beginning = h.core.begin(0, false, h.now)?;
    let forgotten_tx = begun(h.settle(beginning)?)?;
    let forgotten = h.core.commit(forgotten_tx, session, h.now)?;
    h.core.forget(forgotten);
    h.until(|h| h.core.pending_requests() == held)?;
    assert_eq!(h.server.applied(), 1, "the forgotten commit applied");
    assert_eq!(h.core.next_request(session), Some(first + 1));
    let next_beginning = h.core.begin(0, false, h.now)?;
    let next_tx = begun(h.settle(next_beginning)?)?;
    let committing = h.core.commit(next_tx, session, h.now)?;
    let answer = crate::utils::harness::committed(h.settle(committing)?)?;
    assert_eq!(answer.request(), first + 1);
    assert_eq!(h.core.pending_requests(), held);
    Ok(())
}
