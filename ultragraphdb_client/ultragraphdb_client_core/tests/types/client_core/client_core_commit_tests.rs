/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_client_core::{ClientError, ClientErrorEnum, Reply, Session, Transaction};
use ultragraphdb_protocol_transport::{Anonymous, TransportError};
use ultragraphdb_protocol_wire::{
    BodyView, CommitView, ErrorCode, FrameView, MAX_FRAME_SIZE, TermTable,
};

use crate::types::client_core::client_core_tests::counter;
use crate::utils::harness::{
    CLUSTER, Harness, begun, committed, config, dropped, registered, server,
};
use ultragraphdb_tool_bdd::{Context as _, TestResult};

fn session(h: &mut Harness) -> TestResult<Session> {
    let p = h.core.register(h.now)?;
    registered(h.settle(p)?)
}

fn begin(h: &mut Harness) -> TestResult<Transaction> {
    let p = h.core.begin(0, false, h.now)?;
    begun(h.settle(p)?)
}

/// Every Commit frame the server received: its session and request.
fn commits(h: &Harness) -> Vec<(u64, u64)> {
    h.server
        .received()
        .iter()
        .filter_map(|f| {
            FrameView::validate(f, CLUSTER, MAX_FRAME_SIZE)
                .ok()
                .and_then(|v| {
                    CommitView::from_frame(&v)
                        .ok()
                        .map(|c| (c.session(), c.request()))
                })
        })
        .collect()
}

#[test]
fn test_a_commit_answers_its_version_and_results_and_advances_the_session() -> TestResult {
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    let tx = begin(&mut h)?;
    h.core.operation(&tx, &counter(&mut TermTable::new()))?;
    h.core.operation(&tx, &counter(&mut TermTable::new()))?;
    let second = begin(&mut h)?;
    let p = h.core.commit(tx, s, h.now)?;
    assert_eq!(
        h.core.commit(second, s, h.now).err().context("an error")?.0,
        ClientErrorEnum::SessionBusy { session: 9 }
    );
    let c = committed(h.settle(p)?)?;
    assert_eq!((c.version(), c.request(), c.results().len()), (1_000, 1, 2));
    assert_eq!(c.results()[1].uid(), 101);
    assert_eq!(h.core.next_request(s), Some(2));
    let committing = {
        let t = begin(&mut h)?;
        h.core.commit(t, s, h.now)
    }?;
    let next_commit = committed(h.settle(committing)?)?;
    assert_eq!((next_commit.version(), next_commit.request()), (1_002, 2));
    assert_eq!(h.server.applied(), 2);
    assert_eq!(commits(&h), vec![(9, 1), (9, 2)]);
    Ok(())
}

/// The Rust client: the client retries a lost reply through its session.
#[test]
fn test_a_lost_reply_is_retried_through_the_session_and_applied_once() -> TestResult {
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    let tx = begin(&mut h)?;
    h.server.drop_after_next_commit();
    let p = h.core.commit(tx, s, h.now)?;
    let c = committed(h.settle(p)?)?;
    assert_eq!(h.server.applied(), 1, "applied once");
    assert_eq!(h.connections, 2, "the client reconnected");
    assert_eq!(
        commits(&h),
        vec![(9, 1), (9, 1)],
        "the retry carries the same session and request"
    );
    assert_eq!((c.version(), c.request()), (1_000, 1));
    assert_eq!(h.core.next_request(s), Some(2));
    assert_eq!(
        h.core
            .write(
                &tx,
                &ultragraphdb_protocol_wire::Write::new(TermTable::new(), Vec::new())
            )
            .err()
            .context("an error")?
            .0,
        ClientErrorEnum::StreamFinished {
            stream: tx.stream()
        },
        "the old stream is gone"
    );
    Ok(())
}

#[test]
fn test_a_busy_session_is_asked_again_until_the_commit_is_answered() -> TestResult {
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    h.server.drop_after_next_commit();
    h.server.answer_busy_retries(2);
    let p = {
        let t = begin(&mut h)?;
        h.core.commit(t, s, h.now)
    }?;
    let c = committed(h.settle(p)?)?;
    assert_eq!(c.request(), 1);
    assert_eq!(h.server.applied(), 1);
    assert_eq!(
        commits(&h),
        vec![(9, 1); 4],
        "the first send, two busy answers, the answered retry"
    );
    Ok(())
}

#[test]
fn test_retries_are_bounded_and_leave_the_session_in_doubt_until_resolved() -> TestResult {
    // Three attempts, each sent and lost with its connection before its reply: the first send, applied, then two
    // retries, each arriving and lost before it is answered.
    let mut h = Harness::with(
        config().with_commit_attempts(3),
        server(),
        Default::default(),
        1,
        Anonymous,
    );
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    let s = session(&mut h)?;
    let tx = begin(&mut h)?;
    h.server.drop_after_next_commit();
    h.server.lose_next_retries(2);
    let p = h.core.commit(tx, s, h.now)?;
    let answer = h.settle(p)?.err().context("an error")?;
    assert_eq!(
        answer.0,
        ClientErrorEnum::RetriesExhausted {
            session: 9,
            request: 1,
            attempts: 3
        }
    );
    assert_eq!(
        commits(&h),
        vec![(9, 1); 3],
        "the first send and two retries, each sent before its connection was lost"
    );
    assert_eq!(h.connections, 3, "one connection per attempt");
    assert_eq!(
        h.server.applied(),
        1,
        "the first attempt was applied, its reply lost"
    );
    assert!(!h.core.needs_connection(), "nothing waits to be retried");
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    let next_tx = begin(&mut h)?;
    assert_eq!(
        h.core
            .commit(next_tx, s, h.now)
            .err()
            .context("an error")?
            .0,
        ClientErrorEnum::InDoubt {
            session: 9,
            request: 1
        }
    );
    let resolving = h.core.resolve(s, h.now)?;
    let c = committed(h.settle(resolving)?)?;
    assert_eq!(c.request(), 1);
    assert_eq!(h.core.next_request(s), Some(2));
    assert_eq!(h.server.applied(), 1);
    Ok(())
}

#[test]
fn test_a_refused_commit_is_not_retried_and_keeps_its_request_number() -> TestResult {
    let mut h = Harness::ready(server())?;
    let _session: Session = session(&mut h)?;
    let resumed = h.core.resume_session(9, 5);
    let p = {
        let t = begin(&mut h)?;
        h.core.commit(t, resumed, h.now)
    }?;
    let error = h.settle(p)?.err().context("an error")?;
    assert!(
        matches!(
            error.0,
            ClientErrorEnum::Refused {
                code: ErrorCode::RequestOutOfOrder,
                detail: 1,
                ..
            }
        ),
        "{error:?}"
    );
    assert!(!error.retryable());
    assert_eq!(h.core.next_request(resumed), Some(5));
    assert_eq!(commits(&h), vec![(9, 5)]);
    let unknown = h.core.resume_session(4, 1);
    let committing = {
        let t = begin(&mut h)?;
        h.core.commit(t, unknown, h.now)
    }?;
    assert!(matches!(
        h.settle(committing)?.err().context("an error")?.0,
        ClientErrorEnum::Refused {
            code: ErrorCode::SessionEvicted,
            ..
        }
    ));
    assert_eq!(h.core.next_request(resumed), Some(5));
    Ok(())
}

/// The Rust client: a retry of a commit that never arrived runs the transaction again.
#[test]
fn test_a_retry_refused_as_not_committed_keeps_the_request_and_the_rerun_commits_once() -> TestResult
{
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    let tx = begin(&mut h)?;
    h.server.lose_next_commit();
    let p = h.core.commit(tx, s, h.now)?;
    let error = h.settle(p)?.err().context("an error")?;
    assert!(
        matches!(
            error.0,
            ClientErrorEnum::Refused {
                code: ErrorCode::NotCommitted,
                detail: 1,
                ..
            }
        ),
        "{error:?}"
    );
    assert!(
        error.retryable(),
        "the application runs its transaction again"
    );
    assert_eq!(h.server.applied(), 0);
    assert_eq!(
        h.core.next_request(s),
        Some(1),
        "a refusal keeps the number"
    );
    let next_tx = begin(&mut h)?;
    let committing = h.core.commit(next_tx, s, h.now)?;
    let c = committed(h.settle(committing)?)?;
    assert_eq!(c.request(), 1);
    assert_eq!(h.server.applied(), 1, "committed once");
    assert_eq!(
        commits(&h),
        vec![(9, 1); 3],
        "the lost send, the refused retry, the rerun"
    );
    Ok(())
}

/// The Rust client: a retry of an evicted session reports an unknown outcome.
#[test]
fn test_a_retry_refused_as_evicted_reports_an_unknown_outcome_and_is_not_retried() -> TestResult {
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    let tx = begin(&mut h)?;
    h.server.drop_after_next_commit();
    let p = h.core.commit(tx, s, h.now)?;
    h.until(|h| h.connections == 2)?;
    h.server.evict(s.number());
    let error = h.settle(p)?.err().context("an error")?;
    assert_eq!(
        error.0,
        ClientErrorEnum::OutcomeUnknown {
            session: 9,
            request: 1
        }
    );
    assert!(!error.retryable());
    assert_eq!(commits(&h), vec![(9, 1); 2], "one retry, not repeated");
    assert_eq!(
        h.core.next_request(s),
        None,
        "the session is gone from the client"
    );
    let next_tx = begin(&mut h)?;
    assert_eq!(
        h.core
            .commit(next_tx, s, h.now)
            .err()
            .context("an error")?
            .0,
        ClientErrorEnum::UnknownSession { session: 9 }
    );
    Ok(())
}

#[test]
fn test_a_first_commit_refused_as_evicted_is_refused_and_forgets_the_session() -> TestResult {
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    h.server.evict(s.number());
    let tx = begin(&mut h)?;
    let p = h.core.commit(tx, s, h.now)?;
    assert!(matches!(
        h.settle(p)?.err().context("an error")?.0,
        ClientErrorEnum::Refused {
            code: ErrorCode::SessionEvicted,
            ..
        }
    ));
    assert_eq!(h.core.next_request(s), None);
    Ok(())
}

/// A drop through a session is answered with its commit version, and the session's next request is one higher.
#[test]
fn test_a_drop_through_a_session_is_answered_with_its_commit_version() -> TestResult {
    use ultragraphdb_protocol_wire::DropCommand;
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    let p = h
        .core
        .drop_command(DropCommand::graph(0, "http://ex.org/g"), s, h.now)?;
    let dropped = dropped(h.settle(p)?)?;
    assert_eq!((dropped.version(), dropped.request()), (1_000, 1));
    assert!(dropped.results().is_empty());
    assert_eq!(h.core.next_request(s), Some(2));
    assert_eq!(h.server.applied(), 1);
    Ok(())
}

/// A Drop whose reply is lost is sent again through its session, the same Drop with the same request, on the next
/// connection, and applied once (`PROTOCOL.md` §11.5).
#[test]
fn test_a_lost_drop_reply_is_retried_through_the_session_and_applied_once() -> TestResult {
    use ultragraphdb_protocol_wire::{BodyView, DropCommandView};
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    h.server.drop_after_next_commit();
    let p = h.core.drop_command(
        ultragraphdb_protocol_wire::DropCommand::namespace_data(3),
        s,
        h.now,
    )?;
    let dropped = dropped(h.settle(p)?)?;
    assert_eq!(dropped.request(), 1);
    assert_eq!(h.server.applied(), 1, "applied once");
    assert_eq!(h.connections, 2);
    let drops: Vec<(u64, u64, u64)> = h
        .server
        .received()
        .iter()
        .filter_map(|f| {
            let view = FrameView::validate_client(f, CLUSTER, MAX_FRAME_SIZE).ok()?;
            let drop = DropCommandView::from_frame(&view).ok()?;
            Some((drop.namespace(), drop.session(), drop.request()))
        })
        .collect();
    assert_eq!(drops, vec![(3, 9, 1), (3, 9, 1)], "the same Drop, retried");
    Ok(())
}

/// A drop is its session's commit in flight: the session takes no other until it is answered.
#[test]
fn test_a_drop_in_flight_keeps_its_session_busy() -> TestResult {
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    let tx = begin(&mut h)?;
    let p = h
        .core
        .drop_command(ultragraphdb_protocol_wire::DropCommand::all(), s, h.now)?;
    assert_eq!(
        h.core.commit(tx, s, h.now).err().context("an error")?.0,
        ClientErrorEnum::SessionBusy { session: 9 }
    );
    assert_eq!(
        h.core
            .drop_command(ultragraphdb_protocol_wire::DropCommand::all(), s, h.now)
            .err()
            .context("an error")?
            .0,
        ClientErrorEnum::SessionBusy { session: 9 }
    );
    assert!(matches!(h.settle(p)?, Ok(Reply::Dropped(_))));
    Ok(())
}

/// A Drop's reply naming another request than the one sent is no answer of it.
#[test]
fn test_a_drop_reply_of_another_request_is_a_mismatch() -> TestResult {
    use ultragraphdb_protocol_wire::{BodyBuilder, Command, DropCommand, FLAG_LAST, Header};
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    let p = h.core.drop_command(DropCommand::all(), s, h.now)?;
    while h.core.transmit().is_some() {}
    // The session's registration took stream 1; the drop takes stream 2.
    let stream = 2;
    let reply = DropCommand::all()
        .with_session(9, 2)
        .with_commit_version(77)
        .frame(
            Header::new(CLUSTER, Command::Drop)
                .with_stream(stream)
                .with_flags(FLAG_LAST),
        )?;
    let reply = h.numbered(&reply)?;
    h.deliver(&reply)??;
    assert_eq!(
        h.core
            .take_reply(p)
            .context("an answer")?
            .err()
            .context("an error")?
            .0,
        ClientErrorEnum::ReplyMismatch {
            field: "request",
            expected: 1,
            found: 2
        }
    );
    Ok(())
}

/// A commit reply naming another session, or a reply of another command, is no answer of the commit.
#[test]
fn test_a_commit_reply_of_another_session_or_command_is_a_mismatch() -> TestResult {
    use ultragraphdb_protocol_wire::{BodyBuilder, Command, Commit, FLAG_LAST, Header, Register};
    for (reply, expected) in [
        (
            Commit::new(10, 1, 5, Vec::new())
                .frame(Header::new(CLUSTER, Command::Commit).with_flags(FLAG_LAST))?,
            ClientErrorEnum::ReplyMismatch {
                field: "session",
                expected: 9,
                found: 10,
            },
        ),
        (
            Register::new(9)
                .frame(Header::new(CLUSTER, Command::Register).with_flags(FLAG_LAST))?,
            // Commit is command 5, Register 3 (PROTOCOL.md section 3).
            ClientErrorEnum::ReplyMismatch {
                field: "command",
                expected: 5,
                found: 3,
            },
        ),
    ] {
        let mut h = Harness::ready(server())?;
        let s = session(&mut h)?;
        let tx = begin(&mut h)?;
        let p = h.core.commit(tx, s, h.now)?;
        while h.core.transmit().is_some() {}
        let mut reply = reply;
        reply[56..64].copy_from_slice(&tx.stream().to_le_bytes());
        let reply = h.numbered(&reply)?;
        h.deliver(&reply)??;
        assert_eq!(
            h.core
                .take_reply(p)
                .context("an answer")?
                .err()
                .context("an error")?
                .0,
            expected
        );
        assert_eq!(h.core.next_request(s), Some(1), "no commit was answered");
        assert!(h.core.is_ready(), "a mismatch keeps the connection");
    }
    Ok(())
}

/// A session answering busy on every retry: the retries are spaced by the retry delay and bounded by the attempts,
/// then the commit is in doubt.
#[test]
fn test_busy_answers_past_the_attempts_leave_the_commit_in_doubt() -> TestResult {
    let mut h = Harness::with(
        config().with_commit_attempts(4),
        server(),
        Default::default(),
        1,
        Anonymous,
    );
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    let s = session(&mut h)?;
    let tx = begin(&mut h)?;
    h.server.drop_after_next_commit();
    h.server.answer_busy_retries(5);
    let start = h.now;
    let p = h.core.commit(tx, s, h.now)?;
    let answer = h.settle(p)?.err().context("an error")?;
    // Attempt 1 lost with its connection, attempts 2 to 4 answered busy.
    assert_eq!(
        answer.0,
        ClientErrorEnum::RetriesExhausted {
            session: 9,
            request: 1,
            attempts: 4
        }
    );
    assert_eq!(commits(&h), vec![(9, 1); 4]);
    // The retry on the new connection goes at once; each after a busy answer waits the configured 10 ms.
    assert_eq!(h.now - start, std::time::Duration::from_millis(20));
    assert_eq!(h.server.applied(), 1, "the first attempt was applied");
    let next_tx = begin(&mut h)?;
    assert_eq!(
        h.core
            .commit(next_tx, s, h.now)
            .err()
            .context("an error")?
            .0,
        ClientErrorEnum::InDoubt {
            session: 9,
            request: 1
        }
    );
    let resolving = h.core.resolve(s, h.now)?;
    let c = committed(h.settle(resolving)?)?;
    assert_eq!((c.version(), c.request()), (1_000, 1));
    assert_eq!(h.core.next_request(s), Some(2));
    Ok(())
}

#[test]
fn test_only_a_session_in_doubt_is_resolved() -> TestResult {
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    assert_eq!(
        h.core.resolve(s, h.now).err().context("an error")?.0,
        ClientErrorEnum::NotInDoubt { session: 9 }
    );
    let unknown = h.core.resume_session(4, 1);
    let tx = begin(&mut h)?;
    let p = h.core.commit(tx, unknown, h.now)?;
    let _refused: ClientError = h.settle(p)?.err().context("an error")?;
    assert_eq!(
        h.core.resolve(unknown, h.now).err().context("an error")?.0,
        ClientErrorEnum::UnknownSession { session: 4 },
        "the evicted session is forgotten"
    );
    assert_eq!(
        h.core
            .drop_command(
                ultragraphdb_protocol_wire::DropCommand::all(),
                unknown,
                h.now
            )
            .err()
            .context("an error")?
            .0,
        ClientErrorEnum::UnknownSession { session: 4 }
    );
    Ok(())
}

/// A client closing with a commit unanswered: the commit is in doubt, and its session resolves it before it commits
/// again.
#[test]
fn test_a_commit_unanswered_at_close_is_in_doubt_until_resolved() -> TestResult {
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    let tx = begin(&mut h)?;
    let reader = begin(&mut h)?;
    let p = h.core.commit(tx, s, h.now)?;
    let register = h.core.register(h.now)?;
    h.blackhole = true;
    while h.step()? {}
    assert_eq!(h.server.applied(), 1, "applied, its reply lost");
    h.core.close();
    assert!(!h.core.is_connected());
    assert!(
        !h.core.needs_connection(),
        "a commit in doubt waits for no connection"
    );
    assert_eq!(
        h.core
            .take_reply(p)
            .context("an answer")?
            .err()
            .context("an error")?
            .0,
        ClientErrorEnum::InDoubt {
            session: 9,
            request: 1
        }
    );
    assert_eq!(
        h.core
            .take_reply(register)
            .context("an answer")?
            .err()
            .context("an error")?
            .0,
        ClientErrorEnum::ConnectionLost { cause: None }
    );
    assert_eq!(
        h.core
            .drop_command(ultragraphdb_protocol_wire::DropCommand::all(), s, h.now)
            .err()
            .context("an error")?
            .0,
        ClientErrorEnum::InDoubt {
            session: 9,
            request: 1
        }
    );
    assert_eq!(
        h.core.commit(reader, s, h.now).err().context("an error")?.0,
        ClientErrorEnum::InDoubt {
            session: 9,
            request: 1
        }
    );
    h.blackhole = false;
    // Resolved before a connection, it is sent once one is ready.
    let resolving = h.core.resolve(s, h.now)?;
    assert!(h.core.needs_connection());
    let c = committed(h.settle(resolving)?)?;
    assert_eq!((c.version(), c.request()), (1_000, 1));
    assert_eq!(h.server.applied(), 1);
    assert_eq!(h.core.next_request(s), Some(2));
    Ok(())
}

/// A transaction's stream takes one request at a time: while its commit waits for its reply, it takes no other.
#[test]
fn test_a_transaction_awaiting_its_commit_takes_no_other_request() -> TestResult {
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    let other = h.core.resume_session(20, 1);
    let tx = begin(&mut h)?;
    let p = h.core.commit(tx, s, h.now)?;
    let stream = tx.stream();
    assert_eq!(
        h.core
            .write(
                &tx,
                &ultragraphdb_protocol_wire::Write::new(TermTable::new(), Vec::new())
            )
            .err()
            .context("an error")?
            .0,
        ClientErrorEnum::StreamBusy { stream }
    );
    assert_eq!(
        h.core.commit(tx, other, h.now).err().context("an error")?.0,
        ClientErrorEnum::StreamBusy { stream }
    );
    assert_eq!(
        h.core.abort(tx).err().context("an error")?.0,
        ClientErrorEnum::StreamBusy { stream }
    );
    assert_eq!(
        h.core.next_request(other),
        Some(1),
        "the other session untouched"
    );
    assert!(matches!(h.settle(p)?, Ok(Reply::Committed(_))));
    Ok(())
}

/// A drop refused by the session table is not retried, and keeps the session's request number.
#[test]
fn test_a_refused_drop_keeps_its_request_number() -> TestResult {
    use ultragraphdb_protocol_wire::DropCommand;
    let mut h = Harness::ready(server())?;
    let _session: Session = session(&mut h)?;
    let resumed = h.core.resume_session(9, 5);
    let p = h.core.drop_command(DropCommand::all(), resumed, h.now)?;
    let error = h.settle(p)?.err().context("an error")?;
    assert!(
        matches!(
            error.0,
            ClientErrorEnum::Refused {
                code: ErrorCode::RequestOutOfOrder,
                detail: 1,
                ..
            }
        ),
        "{error:?}"
    );
    assert_eq!(h.core.next_request(resumed), Some(5));
    assert_eq!(h.server.applied(), 0);
    // A first drop through an evicted session is refused as such, and forgets the session.
    let evicted = h.core.resume_session(4, 1);
    let committing = h.core.drop_command(DropCommand::all(), evicted, h.now)?;
    assert!(matches!(
        h.settle(committing)?.err().context("an error")?.0,
        ClientErrorEnum::Refused {
            code: ErrorCode::SessionEvicted,
            detail: 4,
            ..
        }
    ));
    assert_eq!(h.core.next_request(evicted), None);
    Ok(())
}

#[test]
fn test_a_drop_before_a_connection_is_refused_as_not_connected() -> TestResult {
    let mut h = Harness::ready(server())?;
    let s = session(&mut h)?;
    h.core.close();
    assert_eq!(
        h.core
            .drop_command(ultragraphdb_protocol_wire::DropCommand::all(), s, h.now)
            .err()
            .context("an error")?
            .0,
        ClientErrorEnum::NotConnected
    );
    // The refusal left the session free: once connected, the drop goes through it as request 1.
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    let p = h
        .core
        .drop_command(ultragraphdb_protocol_wire::DropCommand::all(), s, h.now)?;
    let dropped = dropped(h.settle(p)?)?;
    assert_eq!(dropped.request(), 1);
    Ok(())
}

/// A connection lost before a commit's frame left the client does not use one of the commit's attempts. Here the retry's
/// connection fails in its handshake: two attempts suffice, the first send and the retry.
#[test]
fn test_a_loss_in_the_retrys_handshake_consumes_no_attempt() -> TestResult {
    let mut h = Harness::with(
        config().with_commit_attempts(2),
        server(),
        Default::default(),
        1,
        Anonymous,
    );
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    let s = session(&mut h)?;
    let tx = begin(&mut h)?;
    h.server.drop_after_next_commit();
    let p = h.core.commit(tx, s, h.now)?;
    h.until(|h| h.connections == 2)?;
    // Its Welcome lost: the second connection times out in its handshake, before the retry was sent.
    h.blackhole = true;
    h.until(|h| !h.core.is_connected())?;
    h.blackhole = false;
    assert_eq!(
        h.core.take_reply(p),
        None,
        "the failed handshake used no attempt"
    );
    assert!(
        h.core.needs_connection(),
        "the retry waits for the next connection"
    );
    let c = committed(h.settle(p)?)?;
    assert_eq!(c.request(), 1);
    assert_eq!(commits(&h), vec![(9, 1); 2]);
    assert_eq!(h.server.applied(), 1);
    Ok(())
}

/// The commit's frame still queued when the connection is lost: one attempt, and it is still to be made.
#[test]
fn test_a_loss_with_the_commit_still_queued_consumes_no_attempt() -> TestResult {
    let mut h = Harness::with(
        config().with_commit_attempts(1),
        server(),
        Default::default(),
        1,
        Anonymous,
    );
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    let s = session(&mut h)?;
    let tx = begin(&mut h)?;
    let p = h.core.commit(tx, s, h.now)?;
    h.core.disconnected(TransportError::closed(), h.now);
    assert_eq!(h.core.take_reply(p), None, "not answered");
    assert!(
        h.core.needs_connection(),
        "the commit waits for a connection"
    );
    // Its retry finds no transaction and nothing committed: code 18 (PROTOCOL.md section 11.5).
    let error = h.settle(p)?.err().context("an error")?;
    assert!(
        matches!(
            error.0,
            ClientErrorEnum::Refused {
                code: ErrorCode::NotCommitted,
                detail: 1,
                ..
            }
        ),
        "{error:?}"
    );
    assert_eq!(
        commits(&h),
        vec![(9, 1)],
        "only the retry reached the server"
    );
    assert_eq!(h.server.applied(), 0);
    Ok(())
}

/// Lost while a busy answer's delay runs: the busy answer used its attempt, the loss none.
#[test]
fn test_a_loss_during_a_busy_answers_delay_consumes_no_attempt() -> TestResult {
    let mut h = Harness::with(
        config().with_commit_attempts(3),
        server(),
        Default::default(),
        1,
        Anonymous,
    );
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    let s = session(&mut h)?;
    let tx = begin(&mut h)?;
    h.server.drop_after_next_commit();
    h.server.answer_busy_retries(1);
    let p = h.core.commit(tx, s, h.now)?;
    // The first send lost with its connection (attempt 1), the retry answered busy (attempt 2).
    h.until(|h| commits(h).len() == 2)?;
    while h.step()? {}
    assert_eq!(h.core.take_reply(p), None, "waiting out the delay");
    h.core.disconnected(TransportError::closed(), h.now);
    let c = committed(h.settle(p)?)?;
    assert_eq!(c.request(), 1);
    assert_eq!(commits(&h), vec![(9, 1); 3]);
    assert_eq!(h.server.applied(), 1);
    Ok(())
}

/// A connection lost after a commit's frame left the client uses one of its attempts, whether or not the frame
/// reached the server.
#[test]
fn test_a_loss_after_a_commit_is_sent_consumes_an_attempt() -> TestResult {
    // Handed to the shell, then the connection lost before it was written: one attempt, used.
    let mut h = Harness::with(
        config().with_commit_attempts(1),
        server(),
        Default::default(),
        1,
        Anonymous,
    );
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    let s = session(&mut h)?;
    let tx = begin(&mut h)?;
    let p = h.core.commit(tx, s, h.now)?;
    assert!(
        h.core.transmit().is_some(),
        "the commit's frame leaves the core"
    );
    h.core.disconnected(TransportError::closed(), h.now);
    assert_eq!(
        h.core
            .take_reply(p)
            .context("an answer")?
            .err()
            .context("an error")?
            .0,
        ClientErrorEnum::RetriesExhausted {
            session: 9,
            request: 1,
            attempts: 1
        }
    );
    assert!(!h.core.needs_connection());

    // Arrived and applied, its reply lost with the connection: one attempt, used.
    let mut harness = Harness::with(
        config().with_commit_attempts(1),
        server(),
        Default::default(),
        1,
        Anonymous,
    );
    harness.connect()?;
    harness.until(|harness| harness.core.is_ready())?;
    let session = session(&mut harness)?;
    let next_tx = begin(&mut harness)?;
    harness.server.drop_after_next_commit();
    let committing = harness.core.commit(next_tx, session, harness.now)?;
    assert_eq!(
        harness.settle(committing)?.err().context("an error")?.0,
        ClientErrorEnum::RetriesExhausted {
            session: 9,
            request: 1,
            attempts: 1
        }
    );
    assert_eq!(commits(&harness), vec![(9, 1)]);
    assert_eq!(harness.server.applied(), 1);
    assert_eq!(harness.connections, 1, "nothing is retried");
    Ok(())
}
