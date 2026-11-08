/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The corner cases of the four request types and the one request counter: what each answer does to a session
//! (`PROTOCOL.md` §11.5), a request's token, tenant and principal, what a session's resume refuses.

use ultragraphdb_client_core::{ClientError, ClientErrorEnum, Reply, Session};
use ultragraphdb_core_ids::{Epoch, SessionToken};
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{
    Begin, BeginView, BodyView, Command, CommitOperation, ConditionalWrite, DirectWrite,
    DropCommand, DropCommandView, ErrorCode, FrameView, HeaderView, MAX_FRAME_SIZE, ReadLevel,
    RequestTerm, TermTable, Write, WriteChange, WriteView,
};
use ultragraphdb_tool_bdd::{Context as _, TestResult};

use crate::utils::harness::{
    CLUSTER, Harness, begun, committed, config, dropped, in_doubt, server,
};

fn session(h: &mut Harness) -> TestResult<Session> {
    let p = h.core.register(h.now)?;
    match h.settle(p)? {
        Ok(Reply::Registered(session)) => Ok(session),
        other => Err(ultragraphdb_tool_bdd::TestError::new(format!("{other:?}"))),
    }
}

fn begin(h: &mut Harness) -> TestResult<ultragraphdb_client_core::Transaction> {
    let p = h.core.begin(0, false, h.now)?;
    begun(h.settle(p)?)
}

fn insert() -> DirectWrite {
    let mut terms = TermTable::new();
    let a = terms.iri("http://ex.org/a");
    DirectWrite::new(
        3,
        terms,
        vec![WriteChange::Insert {
            graph: None,
            subject: a,
            predicate: a,
            object: a,
        }],
    )
}

/// An insert-if-absent, its node created.
fn claim() -> ConditionalWrite {
    let mut terms = TermTable::new();
    let predicate = terms.iri("http://ex.org/id");
    let value = terms.iri("http://ex.org/x");
    ConditionalWrite::new(
        3,
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

fn token(epoch: u64) -> TestResult<SessionToken> {
    Ok(SessionToken::new(1, 5, Epoch::new(epoch)?))
}

/// The commands the server received of `command`, each frame's stream.
fn streams_of(h: &Harness, command: Command) -> Vec<u64> {
    h.server
        .received()
        .iter()
        .filter_map(|f| HeaderView::new(f).ok())
        .filter(|header| header.command() == command.number())
        .map(|header| header.stream())
        .collect()
}

fn refused_code(error: &ClientError) -> Option<ErrorCode> {
    if let ClientErrorEnum::Refused { code, .. } = &error.0 {
        return Some(*code);
    }
    None
}

#[test]
fn test_a_session_of_no_number_or_a_request_of_none_is_refused() -> TestResult {
    let mut h = Harness::new(server());
    assert_eq!(
        h.core.resume_session(0, 1).err(),
        Some(ClientError::request(ProtocolError::field_value(
            Command::Commit.number(),
            "session",
            0
        )))
    );
    assert_eq!(
        h.core.resume_session(9, 0).err(),
        Some(ClientError::request(ProtocolError::field_value(
            Command::Commit.number(),
            "request",
            0
        )))
    );
    let resumed = h.core.resume_session(9, 1)?;
    assert_eq!(h.core.next_request(resumed), Some(1));
    Ok(())
}

#[test]
fn test_resuming_a_session_with_a_request_outstanding_is_refused() -> TestResult {
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    let p = h.core.direct_write(insert(), s, h.now)?;
    assert_eq!(
        h.core.resume_session(s.number(), 7).err(),
        Some(ClientError::session_busy(s.number())),
        "in flight"
    );
    assert!(matches!(h.settle(p)?, Ok(Reply::Committed(_))));
    h.server.refuse_next_commit(ErrorCode::Unavailable, 0);
    let p2 = h.core.direct_write(insert(), s, h.now)?;
    assert_eq!(
        in_doubt(&h.settle(p2)?.err().context("in doubt")?),
        Some((s.number(), 2))
    );
    assert_eq!(
        h.core.resume_session(s.number(), 7).err(),
        Some(ClientError::in_doubt(s.number(), 2, None)),
        "in doubt"
    );
    Ok(())
}

/// A stale number is refused by the partition with code 13, and the counter moves above its `detail`, for a Commit, a
/// Drop and a one-frame write alike.
#[test]
fn test_a_new_request_refused_with_code_13_moves_the_counter_above_detail() -> TestResult {
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    h.server
        .refuse_next_commit(ErrorCode::RequestOutOfOrder, 40);
    let tx = begin(&mut h)?;
    let p = h.core.commit(tx, s, h.now)?;
    let error = h.settle(p)?.err().context("refused")?;
    assert_eq!(refused_code(&error), Some(ErrorCode::RequestOutOfOrder));
    assert_eq!(h.core.next_request(s), Some(41));
    h.server
        .refuse_next_commit(ErrorCode::RequestOutOfOrder, 50);
    let p2 = h.core.drop_command(DropCommand::tenant_data(0), s, h.now)?;
    assert!(h.settle(p2)?.is_err());
    assert_eq!(h.core.next_request(s), Some(51));
    h.server
        .refuse_next_commit(ErrorCode::RequestOutOfOrder, u64::MAX);
    let p3 = h.core.direct_write(insert(), s, h.now)?;
    assert!(h.settle(p3)?.is_err());
    assert_eq!(h.core.next_request(s), Some(u64::MAX), "saturated");
    Ok(())
}

/// A resent request refused with code 13 has an unknown outcome for good, and the counter still moves.
#[test]
fn test_a_resend_refused_with_code_13_is_unknown_and_moves_the_counter() -> TestResult {
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    h.server.refuse_next_commit(ErrorCode::Unavailable, 0);
    h.server.refuse_next_commit(ErrorCode::RequestOutOfOrder, 9);
    let p = h.core.direct_write(insert(), s, h.now)?;
    assert!(h.settle(p)?.is_err());
    let resend = h.core.resolve(s, h.now)?;
    assert_eq!(
        h.settle(resend)?.err().map(|e| e.0),
        Some(ClientErrorEnum::OutcomeUnknown {
            session: s.number(),
            request: 1
        })
    );
    assert_eq!(h.core.next_request(s), Some(10));
    assert_eq!(
        h.core.resolve(s, h.now).err(),
        Some(ClientError::not_in_doubt(s.number()))
    );
    Ok(())
}

/// A first request refused with code 12 answers the refusal itself, and the session is gone.
#[test]
fn test_a_first_request_refused_with_code_12_forgets_the_session() -> TestResult {
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    h.server.refuse_next_commit(ErrorCode::SessionEvicted, 0);
    let p = h.core.direct_write(insert(), s, h.now)?;
    let error = h.settle(p)?.err().context("refused")?;
    assert_eq!(refused_code(&error), Some(ErrorCode::SessionEvicted));
    assert_eq!(h.core.next_request(s), None);
    Ok(())
}

/// A one-frame write answered code 14 goes again by itself, the same number on a new stream.
#[test]
fn test_a_one_frame_write_answered_busy_goes_again_on_a_new_stream() -> TestResult {
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    h.server.answer_busy_retries(1);
    let p = h.core.direct_write(insert(), s, h.now)?;
    let answer = committed(h.settle(p)?)?;
    assert_eq!(answer.request(), 1);
    let streams = streams_of(&h, Command::DirectWrite);
    assert_eq!(streams.len(), 2);
    assert!(streams.first() != streams.last(), "{streams:?}");
    Ok(())
}

/// A resend answered 17 past its attempts stays in doubt, its cause the refusal, and can be resolved again.
#[test]
fn test_a_resend_answered_17_past_its_attempts_stays_in_doubt() -> TestResult {
    let mut h = Harness::with(
        config().with_commit_attempts(2),
        server(),
        Default::default(),
        1,
        ultragraphdb_protocol_transport::Anonymous,
    );
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    let s = session(&mut h)?;
    h.server.refuse_next_commit(ErrorCode::Unavailable, 0);
    h.server.refuse_next_commit(ErrorCode::Unavailable, 0);
    h.server.refuse_next_commit(ErrorCode::Unavailable, 0);
    let p = h.core.direct_write(insert(), s, h.now)?;
    assert!(h.settle(p)?.is_err());
    let resend = h.core.resolve(s, h.now)?;
    let error = h.settle(resend)?.err().context("in doubt")?;
    assert_eq!(in_doubt(&error), Some((s.number(), 1)));
    let ClientErrorEnum::InDoubt {
        cause: Some(cause), ..
    } = &error.0
    else {
        return Err(ultragraphdb_tool_bdd::TestError::new(format!("{error:?}")));
    };
    assert_eq!(refused_code(cause), Some(ErrorCode::Unavailable));
    assert_eq!(streams_of(&h, Command::DirectWrite).len(), 3);
    let resolved = h.core.resolve(s, h.now)?;
    assert!(matches!(h.settle(resolved)?, Ok(Reply::Committed(_))));
    Ok(())
}

/// A resent Commit's code 18 says it did not commit, its number the session's next again; any other refusal says nothing
/// of it, and leaves it in doubt.
#[test]
fn test_a_resent_commit_is_decided_by_its_code() -> TestResult {
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    h.server.refuse_next_commit(ErrorCode::Unavailable, 0);
    h.server.refuse_next_commit(ErrorCode::PermissionDenied, 2);
    h.server.refuse_next_commit(ErrorCode::NotCommitted, 1);
    let tx = begin(&mut h)?;
    let p = h.core.commit(tx, s, h.now)?;
    assert!(h.settle(p)?.is_err());
    let resend = h.core.resolve(s, h.now)?;
    let error = h.settle(resend)?.err().context("in doubt")?;
    assert_eq!(in_doubt(&error), Some((s.number(), 1)));
    let resend2 = h.core.resolve(s, h.now)?;
    let error2 = h.settle(resend2)?.err().context("refused")?;
    assert_eq!(refused_code(&error2), Some(ErrorCode::NotCommitted));
    assert_eq!(h.core.next_request(s), Some(1), "the number is reused");
    assert_eq!(
        h.core.resolve(s, h.now).err(),
        Some(ClientError::not_in_doubt(s.number()))
    );
    Ok(())
}

/// A resent one-frame write refused before its session table decides, code 21 or 23 from the frontend, says nothing of it and
/// stays in doubt, its number kept; a refusal the partition gives says it did not commit.
#[test]
fn test_a_resent_one_frame_write_refused_before_its_table_stays_in_doubt() -> TestResult {
    for before_the_table in [ErrorCode::PermissionDenied, ErrorCode::CrossPartition] {
        let mut h = Harness::ready(server())?;
        let s = session(&mut h)?;
        h.server.refuse_next_commit(ErrorCode::Unavailable, 0);
        h.server.refuse_next_commit(before_the_table, 0);
        let p = h.core.direct_write(insert(), s, h.now)?;
        assert!(h.settle(p)?.is_err());
        let resend = h.core.resolve(s, h.now)?;
        let error = h.settle(resend)?.err().context("in doubt")?;
        assert_eq!(
            in_doubt(&error),
            Some((s.number(), 1)),
            "{before_the_table:?}"
        );
        let resolved = h.core.resolve(s, h.now)?;
        assert!(matches!(h.settle(resolved)?, Ok(Reply::Committed(_))));
    }
    Ok(())
}

/// A resent Drop goes as its first did, and answers a Drop's reply.
#[test]
fn test_a_drop_in_doubt_is_resent_as_a_drop() -> TestResult {
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    h.server
        .apply_and_refuse_next_commit(ErrorCode::Unavailable, 0);
    let p = h.core.drop_command(DropCommand::tenant_data(0), s, h.now)?;
    assert_eq!(
        in_doubt(&h.settle(p)?.err().context("in doubt")?),
        Some((s.number(), 1))
    );
    let resend = h.core.resolve(s, h.now)?;
    let answer = dropped(h.settle(resend)?)?;
    assert_eq!(answer.version(), h.server.version());
    assert_eq!(streams_of(&h, Command::Drop).len(), 2);
    assert_eq!(streams_of(&h, Command::Commit).len(), 0);
    assert_eq!(h.core.next_request(s), Some(2));
    assert_eq!(
        h.core.session_token(s),
        answer.token(),
        "a Drop's token is kept"
    );
    Ok(())
}

/// A retry that cannot be sent, its stream refused at the Welcome's limit, is in doubt with that as its cause.
#[test]
fn test_a_retry_refused_at_the_stream_limit_names_it_as_its_cause() -> TestResult {
    let welcome = crate::utils::harness::welcome().with_max_streams(1);
    let mut h = Harness::ready(ultragraphdb_client_core::TestServer::new(CLUSTER, welcome))?;
    let s = session(&mut h)?;
    h.server.refuse_next_commit(ErrorCode::Unavailable, 0);
    let p = h.core.direct_write(insert(), s, h.now)?;
    assert!(h.settle(p)?.is_err());
    let tx = begin(&mut h)?;
    let resend = h.core.resolve(s, h.now)?;
    let error = h
        .core
        .take_reply(resend)
        .context("an answer")?
        .err()
        .context("in doubt")?;
    let ClientErrorEnum::InDoubt {
        cause: Some(cause), ..
    } = &error.0
    else {
        return Err(ultragraphdb_tool_bdd::TestError::new(format!("{error:?}")));
    };
    assert_eq!(**cause, ClientError::stream_limit(1));
    h.core.abort(tx)?;
    Ok(())
}

/// A connection that cannot be made spends a commit's attempts, and its last failure is why the commit is in doubt.
#[test]
fn test_a_connection_not_made_is_the_cause_of_a_commit_in_doubt() -> TestResult {
    let mut h = Harness::with(
        config().with_commit_attempts(2),
        server(),
        Default::default(),
        1,
        ultragraphdb_protocol_transport::Anonymous,
    );
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    let s = session(&mut h)?;
    h.server.drop_after_next_commit();
    let p = h.core.direct_write(insert(), s, h.now)?;
    h.until(|h| !h.core.is_connected())?;
    let cause = ClientError::timed_out();
    h.core.connect_failed(&cause);
    let error = h
        .core
        .take_reply(p)
        .context("an answer")?
        .err()
        .context("in doubt")?;
    assert_eq!(error, ClientError::in_doubt(s.number(), 1, Some(cause)));
    Ok(())
}

/// Through no session there is no resend: a lost reply and a code 17 are unknown, a frame never sent is lost, and any
/// other refusal answers itself.
#[test]
fn test_a_request_through_no_session_has_no_resend() -> TestResult {
    let unknown = ClientErrorEnum::OutcomeUnknown {
        session: 0,
        request: 0,
    };
    let mut h = Harness::ready(server())?;
    h.server.refuse_next_commit(ErrorCode::Unavailable, 0);
    let p = h.core.direct_write(insert(), None, h.now)?;
    assert_eq!(h.settle(p)?.err().map(|e| e.0), Some(unknown.clone()));
    h.server.refuse_next_commit(ErrorCode::Conflict, 1);
    let p2 = h.core.conditional_write(claim(), None, h.now)?;
    let error = h.settle(p2)?.err().context("refused")?;
    assert_eq!(refused_code(&error), Some(ErrorCode::Conflict));
    h.server.drop_after_next_commit();
    let p3 = h
        .core
        .drop_command(DropCommand::tenant_data(0), None, h.now)?;
    assert_eq!(h.settle(p3)?.err().map(|e| e.0), Some(unknown));
    assert!(!h.core.needs_connection());
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    let p4 = h.core.direct_write(insert(), None, h.now)?;
    h.core.close();
    assert_eq!(
        h.core.take_reply(p4),
        Some(Err(ClientError::connection_lost(None))),
        "its frame never left"
    );
    Ok(())
}

/// A frame through no session is numbered 0 with session 0, and a one-frame write always names its tenant.
#[test]
fn test_a_one_frame_write_through_no_session_names_none() -> TestResult {
    let mut h = Harness::ready(server())?;
    let p = h.core.direct_write(insert(), None, h.now)?;
    let answer = committed(h.settle(p)?)?;
    assert_eq!(answer.request(), 0);
    assert_eq!(answer.token(), None);
    let sent = h.server.received().last().context("a frame")?;
    let view = FrameView::validate(sent, CLUSTER, MAX_FRAME_SIZE)?;
    let write = ultragraphdb_protocol_wire::DirectWriteView::from_frame(&view)?;
    assert_eq!(
        (write.session(), write.request(), write.tenant()),
        ([0; 16], 0, 3)
    );
    Ok(())
}

/// A client sends a one-frame write's `principal` as 0: the frontend alone fills it.
#[test]
fn test_a_one_frame_write_naming_a_principal_is_refused_before_it_is_sent() -> TestResult {
    let mut h = Harness::ready(server())?;
    assert_eq!(
        h.core
            .direct_write(insert().with_principal(7), None, h.now)
            .err(),
        Some(ClientError::request(ProtocolError::field_value(
            Command::DirectWrite.number(),
            "principal",
            7
        )))
    );
    assert_eq!(
        h.core
            .conditional_write(claim().with_principal(7), None, h.now)
            .err(),
        Some(ClientError::request(ProtocolError::field_value(
            Command::ConditionalWrite.number(),
            "principal",
            7
        )))
    );
    assert_eq!(h.core.transmit(), None, "nothing queued");
    Ok(())
}

/// A second request of a session waits for the first, and an unknown session is refused, for every request type.
#[test]
fn test_one_request_at_a_time_per_session_across_the_request_types() -> TestResult {
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    let p = h.core.direct_write(insert(), s, h.now)?;
    let busy = Some(ClientError::session_busy(s.number()));
    assert_eq!(h.core.conditional_write(claim(), s, h.now).err(), busy);
    assert_eq!(
        h.core
            .drop_command(DropCommand::tenant_data(0), s, h.now)
            .err(),
        busy
    );
    assert!(matches!(h.settle(p)?, Ok(Reply::Committed(_))));
    assert_eq!(h.core.next_request(s), Some(2), "one counter");
    let p2 = h.core.conditional_write(claim(), s, h.now)?;
    assert_eq!(committed(h.settle(p2)?)?.request(), 2);
    let tx = begin(&mut h)?;
    let p3 = h.core.commit(tx, s, h.now)?;
    assert_eq!(committed(h.settle(p3)?)?.request(), 3);
    Ok(())
}

/// A ConditionalWrite's answer names the node its operation created, by its IRI.
#[test]
fn test_a_conditional_write_names_its_node() -> TestResult {
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    let p = h.core.conditional_write(claim(), s, h.now)?;
    let answer = committed(h.settle(p)?)?;
    assert_eq!(answer.results().len(), 1);
    assert_eq!(answer.node(0), Some("http://ex.org/node/0"));
    assert_eq!(answer.node(1), None);
    assert!(answer.results().first().is_some_and(|r| r.created()));
    Ok(())
}

/// A Begin names its level, deadline and token, and the transaction takes its level.
#[test]
fn test_a_begin_names_its_level_deadline_and_token() -> TestResult {
    let mut h = Harness::ready(server())?;
    let begin = Begin::new(5, 0, true)
        .with_level(ReadLevel::MonotonicFast)
        .with_deadline_ms(70)
        .with_token(token(12)?);
    let p = h.core.begin_with(begin, None, h.now)?;
    let tx = begun(h.settle(p)?)?;
    assert_eq!((tx.level(), tx.tenant()), (ReadLevel::MonotonicFast, 5));
    let sent = h.server.received().last().context("a frame")?;
    let view = FrameView::validate(sent, CLUSTER, MAX_FRAME_SIZE)?;
    let sent2 = BeginView::from_frame(&view)?;
    assert_eq!(sent2.level(), ReadLevel::MonotonicFast);
    assert_eq!(sent2.deadline_ms(), 70);
    assert_eq!(sent2.token(), Some(&token(12)?));
    Ok(())
}

/// A Begin through a session carries the higher epoch of the session's token and its own.
#[test]
fn test_a_begin_through_a_session_carries_the_higher_token() -> TestResult {
    let sent_token = |h: &Harness| -> TestResult<Option<SessionToken>> {
        let sent = h.server.received().last().context("a frame")?;
        let view = FrameView::validate(sent, CLUSTER, MAX_FRAME_SIZE)?;
        Ok(BeginView::from_frame(&view)?.token().cloned())
    };
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    h.core.close();
    assert_eq!(h.core.session_token(s), None);
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    let p = h.core.begin_with(Begin::new(0, 0, true), s, h.now)?;
    let _none = begun(h.settle(p)?)?;
    assert_eq!(sent_token(&h)?, None, "no reply has given it one");
    h.server.set_epoch(12);
    let p2 = h.core.direct_write(insert(), s, h.now)?;
    let held = committed(h.settle(p2)?)?
        .token()
        .cloned()
        .context("a token")?;
    assert_eq!(h.core.session_token(s), Some(&held));
    let p3 = h.core.begin_with(Begin::new(0, 0, true), s, h.now)?;
    let _carrying = begun(h.settle(p3)?)?;
    assert_eq!(sent_token(&h)?, Some(held.clone()));
    let lower = Begin::new(0, 0, true).with_token(token(3)?);
    let p4 = h.core.begin_with(lower, s, h.now)?;
    let _higher_held = begun(h.settle(p4)?)?;
    assert_eq!(sent_token(&h)?, Some(held));
    let higher = Begin::new(0, 0, true).with_token(token(20)?);
    let p5 = h.core.begin_with(higher, s, h.now)?;
    let _own_higher = begun(h.settle(p5)?)?;
    assert_eq!(sent_token(&h)?, Some(token(20)?));
    Ok(())
}

/// An equal epoch keeps the token held: only a higher one replaces it.
#[test]
fn test_a_token_of_an_equal_or_lower_epoch_replaces_nothing() -> TestResult {
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    h.server.set_epoch(12);
    h.server.set_position(40);
    let p = h.core.direct_write(insert(), s, h.now)?;
    let first = committed(h.settle(p)?)?
        .token()
        .cloned()
        .context("a token")?;
    h.server.set_position(41);
    let p2 = h.core.direct_write(insert(), s, h.now)?;
    let second = committed(h.settle(p2)?)?
        .token()
        .cloned()
        .context("a token")?;
    assert_eq!(first.epoch(), second.epoch());
    assert_eq!(h.core.session_token(s), Some(&first));
    Ok(())
}

/// A DirectWrite holding a value the frontend refuses is refused like a Commit: code 7 answers itself, its value kept.
#[test]
fn test_a_refused_one_frame_write_keeps_its_number_and_its_value() -> TestResult {
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    h.server
        .fail_next_condition(RequestTerm::Iri("http://ex.org/held".into()));
    let p = h.core.conditional_write(claim(), s, h.now)?;
    let error = h.settle(p)?.err().context("refused")?;
    assert_eq!(refused_code(&error), Some(ErrorCode::OperationFailed));
    assert_eq!(h.core.next_request(s), Some(1));
    Ok(())
}

/// The value a refused compare-and-set's error holds, and whether the client says it was cut.
fn found(error: &ClientError) -> TestResult<(Option<RequestTerm>, bool)> {
    let ClientErrorEnum::Refused { value, .. } = &error.0 else {
        return Err(ultragraphdb_tool_bdd::TestError::new(format!("{error:?}")));
    };
    Ok((value.clone(), error.value_cut()))
}

/// A value found past what the Error frame leaves it arrives cut, and the client says so; one at that limit arrives whole,
/// and the client says nothing was cut (`PROTOCOL.md` §9.4).
#[test]
fn test_a_value_found_past_the_frame_arrives_cut_and_flagged() -> TestResult {
    use ultragraphdb_protocol_wire::MAX_FRAME_FLOOR;
    let literal = |lexical: String| RequestTerm::Literal {
        lexical,
        datatype: None,
    };
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    let whole = "v".repeat(MAX_FRAME_FLOOR);
    h.server.fail_next_condition(literal(whole.clone()));
    let p = h.core.conditional_write(claim(), s, h.now)?;
    let past = h.settle(p)?.err().context("refused")?;
    assert_eq!(refused_code(&past), Some(ErrorCode::OperationFailed));
    let (cut, flagged) = found(&past)?;
    assert!(flagged, "the client says the value was cut");
    let Some(RequestTerm::Literal { lexical, .. }) = cut else {
        return Err(ultragraphdb_tool_bdd::TestError::new(format!("{past:?}")));
    };
    assert!(lexical.len() < whole.len() && whole.starts_with(lexical.as_str()));
    let at = literal(lexical);
    h.server.fail_next_condition(at.clone());
    let p2 = h.core.conditional_write(claim(), s, h.now)?;
    let limit = h.settle(p2)?.err().context("refused")?;
    assert_eq!(
        found(&limit)?,
        (Some(at), false),
        "a value at the limit arrives whole"
    );
    Ok(())
}

/// A statement holding a statement ID is named by its reifier's skolem IRI, one with none by its quad: a write sends what
/// it names, no statement ID.
#[test]
fn test_a_statement_is_named_by_its_quad_or_its_reifier() -> TestResult {
    let mut h = Harness::ready(server())?;
    let tx = begin(&mut h)?;
    let mut terms = TermTable::new();
    let reifier = terms.iri("https://w3id.org/ultragraphdb/tenant/0/.well-known/genid/7-9");
    let subject = terms.iri("http://ex.org/a");
    let changes = vec![
        WriteChange::DeleteStatement {
            graph: None,
            reifier,
        },
        WriteChange::Delete {
            graph: None,
            subject,
            predicate: subject,
            object: subject,
        },
    ];
    h.core.write(&tx, &Write::new(terms, changes.clone()))?;
    while h.step()? {}
    let sent = h.server.received().last().context("a frame")?;
    let view = FrameView::validate(sent, CLUSTER, MAX_FRAME_SIZE)?;
    let write = WriteView::from_frame(&view)?;
    assert_eq!(write.change(0).as_ref(), changes.first());
    assert_eq!(write.change(1).as_ref(), changes.get(1));
    Ok(())
}

/// A tenant is named per request: one connection's Begins, Prepares, Describes, Drops and one-frame writes name several.
#[test]
fn test_each_request_names_its_tenant() -> TestResult {
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    let p = h.core.begin(5, true, h.now)?;
    let five = begun(h.settle(p)?)?;
    let beginning = h.core.begin(6, true, h.now)?;
    let six = begun(h.settle(beginning)?)?;
    assert_eq!((five.tenant(), six.tenant()), (5, 6));
    let describing = h.core.describe(7, h.now)?;
    assert!(h.settle(describing)?.is_ok());
    let dropping = h.core.drop_command(DropCommand::tenant_data(8), s, h.now)?;
    assert!(h.settle(dropping)?.is_ok());
    let sent = h.server.received().last().context("a frame")?;
    let view = FrameView::validate(sent, CLUSTER, MAX_FRAME_SIZE)?;
    assert_eq!(DropCommandView::from_frame(&view)?.tenant(), 8);
    let writing = h.core.direct_write(insert(), s, h.now)?;
    assert!(h.settle(writing)?.is_ok());
    let written = h.server.received().last().context("a frame")?;
    let write_view = FrameView::validate(written, CLUSTER, MAX_FRAME_SIZE)?;
    assert_eq!(
        ultragraphdb_protocol_wire::DirectWriteView::from_frame(&write_view)?.tenant(),
        3
    );
    Ok(())
}

/// A session resumed again keeps the token it holds: the highest epoch is the session's, whatever its number.
#[test]
fn test_resuming_a_session_keeps_its_token() -> TestResult {
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    h.server.set_epoch(12);
    let p = h.core.direct_write(insert(), s, h.now)?;
    let held = committed(h.settle(p)?)?
        .token()
        .cloned()
        .context("a token")?;
    let resumed = h.core.resume_session(s.number(), 9)?;
    assert_eq!(h.core.next_request(resumed), Some(9));
    assert_eq!(h.core.session_token(resumed), Some(&held));
    Ok(())
}

/// A Begin's own token of the very epoch the session's token holds is the one it carries: the session's token takes its
/// place only when of a higher epoch.
#[test]
fn test_an_own_token_of_the_sessions_epoch_is_carried() -> TestResult {
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    h.server.set_epoch(12);
    let p = h.core.direct_write(insert(), s, h.now)?;
    let held = committed(h.settle(p)?)?
        .token()
        .cloned()
        .context("a token")?;
    let own = SessionToken::new(held.partition() + 1, held.position() + 1, held.epoch());
    let begin = Begin::new(0, 0, true).with_token(own.clone());
    let p2 = h.core.begin_with(begin, s, h.now)?;
    let _tx = begun(h.settle(p2)?)?;
    let sent = h.server.received().last().context("a frame")?;
    let view = FrameView::validate(sent, CLUSTER, MAX_FRAME_SIZE)?;
    assert_eq!(BeginView::from_frame(&view)?.token(), Some(&own));
    Ok(())
}

/// A Begin through a session this client does not hold is refused before it is sent.
#[test]
fn test_a_begin_through_a_session_not_held_is_refused() -> TestResult {
    let mut other = Harness::ready(server())?;
    let foreign = session(&mut other)?;
    let mut h = Harness::ready(server())?;
    assert_eq!(
        h.core
            .begin_with(Begin::new(0, 0, true), foreign, h.now)
            .err(),
        Some(ClientError::unknown_session(foreign.number()))
    );
    assert_eq!(h.core.transmit(), None, "nothing is sent");
    Ok(())
}

/// A transaction handed by another client names a stream of this one only by chance: on a stream holding no
/// transaction it is refused as finished.
#[test]
fn test_a_transaction_of_another_client_is_finished() -> TestResult {
    let mut other = Harness::ready(server())?;
    let tx = begin(&mut other)?;
    let mut h = Harness::ready(server())?;
    let _registering = h.core.register(h.now)?;
    assert_eq!(
        h.core.abort(tx).err(),
        Some(ClientError::stream_finished(tx.stream()))
    );
    Ok(())
}
