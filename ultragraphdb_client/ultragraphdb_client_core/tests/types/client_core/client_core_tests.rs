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
        (tx.namespace(), tx.read_version(), tx.read_only()),
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
        (statement.id(), statement.schema(), statement.namespace()),
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

/// A commit reply naming another request than the one sent is no answer of it.
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
            ClientErrorEnum::ReplyMismatch {
                field,
                expected: 1,
                found: 2
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
