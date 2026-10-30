/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use ultragraphdb_client_core::{ClientCore, ClientErrorEnum, Reply, TestServer};
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_transport::{Anonymous, BufferPool, ClientHandshake, NetworkFaults};
use ultragraphdb_protocol_wire::{
    BodyBuilder, ColumnType, Command, CommitOperation, ErrorCode, ErrorReply, FrameView, Header,
    HeaderView, MAX_FRAME_SIZE, Operation, PatternSlot, Prepare, QuadPattern, TermTable, Welcome,
    Write,
};

use crate::utils::harness::{CLUSTER, Harness, RELEASE, UNIX, config, received, server, welcome};

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
fn test_connect_sends_the_hello_and_a_welcome_makes_it_ready() {
    let mut core = ClientCore::new(config(), Anonymous);
    assert!(!core.is_ready());
    assert_eq!(core.transmit(), None);
    core.connect(crate::utils::harness::start(), UNIX).unwrap();
    let hello = core.transmit().unwrap();
    assert_eq!(
        hello,
        ClientHandshake::new(CLUSTER, config().hello())
            .hello_frame()
            .unwrap()
    );
    assert_eq!(HeaderView::new(&hello).unwrap().release(), RELEASE);
    let h = Harness::ready(server());
    assert_eq!(h.core.welcome(), Some(&welcome()));
    assert_eq!(h.core.next_deadline(), None, "nothing is awaited");
}

#[test]
fn test_a_welcome_outside_the_range_closes_the_connection() {
    let mut core = ClientCore::new(config(), Anonymous);
    let now = crate::utils::harness::start();
    core.connect(now, UNIX).unwrap();
    let _ = core.transmit();
    let pool = BufferPool::new(2, 1 << 20);
    let other = Welcome::new(2, 0, "s")
        .with_max_frame(1 << 20)
        .with_max_streams(1)
        .with_initial_credit(1)
        .frame(Header::new(CLUSTER, Command::Welcome).with_sequence(1))
        .unwrap();
    let refused = core
        .receive(received(&pool, &other).unwrap(), now)
        .unwrap_err();
    assert!(
        matches!(refused.0, ClientErrorEnum::Transport { .. }),
        "{refused:?}"
    );
    assert!(!core.is_ready());
}

#[test]
fn test_requests_before_the_welcome_are_refused_as_not_connected() {
    let mut core = ClientCore::new(config(), Anonymous);
    let now = crate::utils::harness::start();
    assert_eq!(
        core.register(now).unwrap_err().0,
        ClientErrorEnum::NotConnected
    );
    core.connect(now, UNIX).unwrap();
    assert_eq!(
        core.begin(0, false, now).unwrap_err().0,
        ClientErrorEnum::NotConnected
    );
}

#[test]
fn test_register_and_begin_answer_a_session_and_a_snapshot() {
    let mut h = Harness::ready(server());
    let p = h.core.register(h.now).unwrap();
    let Ok(Reply::Registered(session)) = h.settle(p) else {
        panic!("a session")
    };
    assert_eq!(session.number(), 9);
    assert_eq!(h.core.next_request(session), Some(1));
    let p = h.core.begin(3, true, h.now).unwrap();
    let Ok(Reply::Begun(tx)) = h.settle(p) else {
        panic!("a transaction")
    };
    assert_eq!(
        (tx.namespace(), tx.read_version(), tx.read_only()),
        (3, 998, true)
    );
    assert_eq!(h.core.take_reply(p), None, "an answer is taken once");
}

#[test]
fn test_a_read_only_transaction_neither_writes_nor_commits() {
    let mut h = Harness::ready(server());
    let p = h.core.register(h.now).unwrap();
    let Ok(Reply::Registered(session)) = h.settle(p) else {
        panic!()
    };
    let p = h.core.begin(0, true, h.now).unwrap();
    let Ok(Reply::Begun(tx)) = h.settle(p) else {
        panic!()
    };
    let stream = tx.stream();
    let write = Write::new(TermTable::new(), Vec::new());
    assert_eq!(
        h.core.write(&tx, &write).unwrap_err().0,
        ClientErrorEnum::ReadOnly { stream }
    );
    assert_eq!(
        h.core
            .operation(&tx, &counter(&mut TermTable::new()))
            .unwrap_err()
            .0,
        ClientErrorEnum::ReadOnly { stream }
    );
    assert_eq!(
        h.core.commit(tx, session, h.now).unwrap_err().0,
        ClientErrorEnum::ReadOnly { stream }
    );
    h.core.abort(tx).unwrap();
    h.until(|h| h.server.received().len() >= 4);
    assert_eq!(
        HeaderView::new(h.server.received().last().unwrap())
            .unwrap()
            .command(),
        Command::Abort.number()
    );
    assert_eq!(
        h.core.write(&tx, &write).unwrap_err().0,
        ClientErrorEnum::StreamFinished { stream },
        "aborted"
    );
}

#[test]
fn test_prepare_describe_and_a_stale_statement() {
    let mut h = Harness::ready(server());
    let mut terms = TermTable::new();
    let name = terms.iri("http://ex.org/name");
    let pattern = QuadPattern::new(
        PatternSlot::DefaultGraph,
        PatternSlot::Parameter(0),
        PatternSlot::Bound(name),
        PatternSlot::Unbound,
    );
    let prepare = Prepare::new(0, 0, 0, terms, pattern, vec![ColumnType::Node], Vec::new());
    let p = h.core.prepare(&prepare, h.now).unwrap();
    let Ok(Reply::Prepared(statement)) = h.settle(p) else {
        panic!("a statement")
    };
    assert_eq!(
        (statement.id(), statement.schema(), statement.namespace()),
        (1, 1, 0)
    );
    assert_eq!(statement.parameter_types(), &[ColumnType::Node]);
    assert!(statement.is_current(1) && !statement.is_current(2));
    let p = h.core.describe(0, h.now).unwrap();
    let Ok(Reply::Described(describe)) = h.settle(p) else {
        panic!("a description")
    };
    assert_eq!(describe.read_version(), 998);
}

#[test]
fn test_a_malformed_reply_closes_the_connection_and_loses_its_requests() {
    let mut h = Harness::ready(server());
    let p = h.core.describe(0, h.now).unwrap();
    while h.core.transmit().is_some() {}
    let mut frame = ultragraphdb_protocol_wire::Describe::new(0, 5)
        .frame(Header::new(CLUSTER, Command::Describe).with_stream(1))
        .unwrap();
    let last = frame.len() - 1;
    frame[last] ^= 4;
    let frame = h.numbered(&frame);
    let refused = h.deliver(&frame);
    let Err(error) = refused else {
        panic!("refused")
    };
    assert!(
        matches!(error.0, ClientErrorEnum::Protocol { error: ref e } if matches!(e.0, ProtocolErrorEnum::Chunk { index: 0, .. })),
        "{error:?}"
    );
    assert!(!h.core.is_ready());
    assert_eq!(
        h.core.take_reply(p).unwrap().unwrap_err().0,
        ClientErrorEnum::ConnectionLost
    );
}

#[test]
fn test_an_error_on_stream_zero_closes_the_connection() {
    let mut h = Harness::ready(server());
    let frame = ErrorReply::new(ErrorCode::Protocol, "bad frame")
        .frame(Header::new(CLUSTER, Command::Error))
        .unwrap();
    let frame = h.numbered(&frame);
    let error = h.deliver(&frame).unwrap_err();
    assert_eq!(
        error.0,
        ClientErrorEnum::Closed {
            code: ErrorCode::Protocol,
            message: "bad frame".into()
        }
    );
    assert!(!h.core.is_ready());
}

#[test]
fn test_an_unanswered_request_times_out_and_closes_the_connection() {
    let mut h = Harness::ready(server());
    h.blackhole = true;
    let p = h.core.register(h.now).unwrap();
    while h.step() {}
    let deadline = h.core.next_deadline().unwrap();
    assert_eq!(deadline, h.now + Duration::from_secs(1));
    h.core.handle_timeout(deadline - Duration::from_millis(1));
    assert_eq!(h.core.take_reply(p), None, "not yet");
    h.core.handle_timeout(deadline);
    assert_eq!(
        h.core.take_reply(p).unwrap().unwrap_err().0,
        ClientErrorEnum::TimedOut
    );
    assert!(!h.core.is_ready());
    assert_eq!(h.core.next_deadline(), None);
}

#[test]
fn test_streams_beyond_the_welcomes_limit_are_refused() {
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
    h.connect();
    h.until(|h| h.core.is_ready());
    let p = h.core.begin(0, false, h.now).unwrap();
    assert_eq!(
        h.core.begin(0, false, h.now).unwrap_err().0,
        ClientErrorEnum::StreamLimit { limit: 1 }
    );
    let Ok(Reply::Begun(_)) = h.settle(p) else {
        panic!()
    };
}

#[test]
fn test_a_request_past_the_welcomes_frame_is_refused_before_it_is_sent() {
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
    h.connect();
    h.until(|h| h.core.is_ready());
    let p = h.core.begin(0, false, h.now).unwrap();
    let Ok(Reply::Begun(tx)) = h.settle(p) else {
        panic!()
    };
    let mut terms = TermTable::new();
    let big = "x".repeat(530_000);
    let _ = terms.iri(&big);
    let error = h
        .core
        .write(&tx, &Write::new(terms, Vec::new()))
        .unwrap_err();
    assert!(
        matches!(error.0, ClientErrorEnum::Request { error: ref e } if matches!(e.0, ProtocolErrorEnum::FrameTooLarge { limit: 524_288, .. })),
        "{error:?}"
    );
}

/// Frames are numbered: a reply repeated is out of sequence, and the connection is given up after its first copy
/// was taken.
#[test]
fn test_a_repeated_reply_is_refused_after_its_first_is_taken() {
    let mut h = Harness::ready(server());
    let p = h.core.register(h.now).unwrap();
    while h.core.transmit().is_some() {}
    let frame = ultragraphdb_protocol_wire::Register::new(77)
        .frame(Header::new(CLUSTER, Command::Register).with_stream(1))
        .unwrap();
    let frame = h.numbered(&frame);
    h.deliver(&frame).unwrap();
    let refused = h.deliver(&frame).unwrap_err();
    assert!(
        matches!(&refused.0, ClientErrorEnum::Protocol { error } if matches!(error.0, ProtocolErrorEnum::SequenceOrder { .. })),
        "{refused:?}"
    );
    let Some(Ok(Reply::Registered(session))) = h.core.take_reply(p) else {
        panic!()
    };
    assert_eq!(session.number(), 77);
    assert!(!h.core.is_ready());
    assert_eq!(
        FrameView::validate(&frame, CLUSTER, MAX_FRAME_SIZE)
            .unwrap()
            .command(),
        Command::Register
    );
}

/// A commit reply naming another request than the one sent is no answer of it.
#[test]
fn test_a_commit_reply_of_another_request_is_a_mismatch() {
    let mut h = Harness::ready(server());
    let p = h.core.register(h.now).unwrap();
    let Ok(Reply::Registered(session)) = h.settle(p) else {
        panic!()
    };
    let p = h.core.begin(0, false, h.now).unwrap();
    let Ok(Reply::Begun(tx)) = h.settle(p) else {
        panic!()
    };
    let p = h.core.commit(tx, session, h.now).unwrap();
    while h.core.transmit().is_some() {}
    {
        let (field, reply) = (
            "request",
            ultragraphdb_protocol_wire::Commit::new(9, 2, 5, Vec::new()),
        );
        let frame = reply
            .frame(Header::new(CLUSTER, Command::Commit).with_stream(tx.stream()))
            .unwrap();
        let frame = h.numbered(&frame);
        h.deliver(&frame).unwrap();
        assert_eq!(
            h.core.take_reply(p).unwrap().unwrap_err().0,
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
}

/// A Write has no reply: its refusal ends the transaction, and is the answer of the transaction's next request.
#[test]
fn test_a_refused_write_is_the_answer_of_the_next_request() {
    let mut h = Harness::ready(server());
    let p = h.core.begin(0, false, h.now).unwrap();
    let Ok(Reply::Begun(tx)) = h.settle(p) else {
        panic!()
    };
    h.core
        .write(&tx, &Write::new(TermTable::new(), Vec::new()))
        .unwrap();
    while h.core.transmit().is_some() {}
    let frame = ErrorReply::new(ErrorCode::Refused, "the catalog refuses")
        .frame(
            Header::new(CLUSTER, Command::Error)
                .with_stream(tx.stream())
                .with_flags(ultragraphdb_protocol_wire::FLAG_LAST),
        )
        .unwrap();
    let frame = h.numbered(&frame);
    h.deliver(&frame).unwrap();
    let refused = h
        .core
        .write(&tx, &Write::new(TermTable::new(), Vec::new()))
        .unwrap_err();
    assert!(
        matches!(refused.0, ClientErrorEnum::Refused { code: ErrorCode::Refused, ref message, .. } if message == "the catalog refuses"),
        "{refused:?}"
    );
    assert_eq!(
        h.core
            .write(&tx, &Write::new(TermTable::new(), Vec::new()))
            .unwrap_err()
            .0,
        ClientErrorEnum::StreamFinished {
            stream: tx.stream()
        }
    );
    assert!(h.core.is_ready());
}
