/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_client_core::{ClientErrorEnum, Reply, Session, Transaction};
use ultragraphdb_protocol_transport::Anonymous;
use ultragraphdb_protocol_wire::{
    BodyView, CommitView, ErrorCode, FrameView, MAX_FRAME_SIZE, TermTable,
};

use crate::types::client_core::client_core_tests::counter;
use crate::utils::harness::{CLUSTER, Harness, config, server};

fn session(h: &mut Harness) -> Session {
    let p = h.core.register(h.now).unwrap();
    let Ok(Reply::Registered(session)) = h.settle(p) else {
        panic!("a session")
    };
    session
}

fn begin(h: &mut Harness) -> Transaction {
    let p = h.core.begin(0, false, h.now).unwrap();
    let Ok(Reply::Begun(tx)) = h.settle(p) else {
        panic!("a transaction")
    };
    tx
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
fn test_a_commit_answers_its_version_and_results_and_advances_the_session() {
    let mut h = Harness::ready(server());
    let s = session(&mut h);
    let tx = begin(&mut h);
    h.core
        .operation(&tx, &counter(&mut TermTable::new()))
        .unwrap();
    h.core
        .operation(&tx, &counter(&mut TermTable::new()))
        .unwrap();
    let second = begin(&mut h);
    let p = h.core.commit(tx, s, h.now).unwrap();
    assert_eq!(
        h.core.commit(second, s, h.now).unwrap_err().0,
        ClientErrorEnum::SessionBusy { session: 9 }
    );
    let Ok(Reply::Committed(c)) = h.settle(p) else {
        panic!("committed")
    };
    assert_eq!((c.version(), c.request(), c.results().len()), (1_000, 1, 2));
    assert_eq!(c.results()[1].uid(), 101);
    assert_eq!(h.core.next_request(s), Some(2));
    let p = {
        let t = begin(&mut h);
        h.core.commit(t, s, h.now)
    }
    .unwrap();
    let Ok(Reply::Committed(c)) = h.settle(p) else {
        panic!()
    };
    assert_eq!((c.version(), c.request()), (1_002, 2));
    assert_eq!(h.server.applied(), 2);
    assert_eq!(commits(&h), vec![(9, 1), (9, 2)]);
}

/// The Rust client: the client retries a lost reply through its session.
#[test]
fn test_a_lost_reply_is_retried_through_the_session_and_applied_once() {
    let mut h = Harness::ready(server());
    let s = session(&mut h);
    let tx = begin(&mut h);
    h.server.drop_after_next_commit();
    let p = h.core.commit(tx, s, h.now).unwrap();
    let Ok(Reply::Committed(c)) = h.settle(p) else {
        panic!("committed")
    };
    assert_eq!(h.server.applied(), 1, "applied once");
    assert_eq!(h.connections, 2, "the client reconnected");
    assert_eq!(
        commits(&h),
        vec![(9, 1), (9, 1)],
        "the retry carries the same session and request"
    );
    assert_eq!((c.version(), c.request()), (1_000, 1));
    assert_eq!(h.core.next_request(s), Some(2));
    assert!(
        !h.core
            .write(
                &tx,
                &ultragraphdb_protocol_wire::Write::new(TermTable::new(), Vec::new())
            )
            .is_ok(),
        "the old stream is gone"
    );
}

#[test]
fn test_a_busy_session_is_asked_again_until_the_commit_is_answered() {
    let mut h = Harness::ready(server());
    let s = session(&mut h);
    h.server.drop_after_next_commit();
    h.server.answer_busy_retries(2);
    let p = {
        let t = begin(&mut h);
        h.core.commit(t, s, h.now)
    }
    .unwrap();
    let Ok(Reply::Committed(c)) = h.settle(p) else {
        panic!("committed")
    };
    assert_eq!(c.request(), 1);
    assert_eq!(h.server.applied(), 1);
    assert_eq!(
        commits(&h),
        vec![(9, 1); 4],
        "the first send, two busy answers, the answered retry"
    );
}

#[test]
fn test_retries_are_bounded_and_leave_the_session_in_doubt_until_resolved() {
    let mut h = Harness::with(
        config().with_commit_attempts(3),
        server(),
        Default::default(),
        1,
        Anonymous,
    );
    h.connect();
    h.until(|h| h.core.is_ready());
    let s = session(&mut h);
    let tx = begin(&mut h);
    h.blackhole = true;
    let p = h.core.commit(tx, s, h.now).unwrap();
    let answer = h.settle(p).unwrap_err();
    assert_eq!(
        answer.0,
        ClientErrorEnum::RetriesExhausted {
            session: 9,
            request: 1,
            attempts: 3
        }
    );
    assert_eq!(
        commits(&h).len(),
        1,
        "the reconnects' Welcomes were lost too, so only the first send arrived"
    );
    assert_eq!(
        h.server.applied(),
        1,
        "the first attempt was applied, its reply lost"
    );
    h.blackhole = false;
    if !h.core.is_ready() {
        h.connect();
        h.until(|h| h.core.is_ready());
    }
    let tx = begin(&mut h);
    assert_eq!(
        h.core.commit(tx, s, h.now).unwrap_err().0,
        ClientErrorEnum::InDoubt {
            session: 9,
            request: 1
        }
    );
    let p = h.core.resolve(s, h.now).unwrap();
    let Ok(Reply::Committed(c)) = h.settle(p) else {
        panic!("resolved from the record")
    };
    assert_eq!(c.request(), 1);
    assert_eq!(h.core.next_request(s), Some(2));
    assert_eq!(h.server.applied(), 1);
}

#[test]
fn test_a_refused_commit_is_not_retried_and_keeps_its_request_number() {
    let mut h = Harness::ready(server());
    let _ = session(&mut h);
    let resumed = h.core.resume_session(9, 5);
    let p = {
        let t = begin(&mut h);
        h.core.commit(t, resumed, h.now)
    }
    .unwrap();
    let error = h.settle(p).unwrap_err();
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
    let p = {
        let t = begin(&mut h);
        h.core.commit(t, unknown, h.now)
    }
    .unwrap();
    assert!(matches!(
        h.settle(p).unwrap_err().0,
        ClientErrorEnum::Refused {
            code: ErrorCode::SessionEvicted,
            ..
        }
    ));
    assert_eq!(h.core.next_request(resumed), Some(5));
}

/// The Rust client: a retry of a commit that never arrived runs the transaction again.
#[test]
fn test_a_retry_refused_as_not_committed_keeps_the_request_and_the_rerun_commits_once() {
    let mut h = Harness::ready(server());
    let s = session(&mut h);
    let tx = begin(&mut h);
    h.server.lose_next_commit();
    let p = h.core.commit(tx, s, h.now).unwrap();
    let error = h.settle(p).unwrap_err();
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
    let tx = begin(&mut h);
    let p = h.core.commit(tx, s, h.now).unwrap();
    let Ok(Reply::Committed(c)) = h.settle(p) else {
        panic!("the rerun commits")
    };
    assert_eq!(c.request(), 1);
    assert_eq!(h.server.applied(), 1, "committed once");
    assert_eq!(
        commits(&h),
        vec![(9, 1); 3],
        "the lost send, the refused retry, the rerun"
    );
}

/// The Rust client: a retry of an evicted session reports an unknown outcome.
#[test]
fn test_a_retry_refused_as_evicted_reports_an_unknown_outcome_and_is_not_retried() {
    let mut h = Harness::ready(server());
    let s = session(&mut h);
    let tx = begin(&mut h);
    h.server.drop_after_next_commit();
    let p = h.core.commit(tx, s, h.now).unwrap();
    h.until(|h| h.connections == 2);
    h.server.evict(s.number());
    let error = h.settle(p).unwrap_err();
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
    let tx = begin(&mut h);
    assert_eq!(
        h.core.commit(tx, s, h.now).unwrap_err().0,
        ClientErrorEnum::UnknownSession { session: 9 }
    );
}

#[test]
fn test_a_first_commit_refused_as_evicted_is_refused_and_forgets_the_session() {
    let mut h = Harness::ready(server());
    let s = session(&mut h);
    h.server.evict(s.number());
    let tx = begin(&mut h);
    let p = h.core.commit(tx, s, h.now).unwrap();
    assert!(matches!(
        h.settle(p).unwrap_err().0,
        ClientErrorEnum::Refused {
            code: ErrorCode::SessionEvicted,
            ..
        }
    ));
    assert_eq!(h.core.next_request(s), None);
}

/// A drop through a session is answered with its commit version, and the session's next request is one higher.
#[test]
fn test_a_drop_through_a_session_is_answered_with_its_commit_version() {
    use ultragraphdb_protocol_wire::DropCommand;
    let mut h = Harness::ready(server());
    let s = session(&mut h);
    let p = h
        .core
        .drop_command(DropCommand::graph(0, "http://ex.org/g"), s, h.now)
        .unwrap();
    let Ok(Reply::Dropped(dropped)) = h.settle(p) else {
        panic!("dropped")
    };
    assert_eq!((dropped.version(), dropped.request()), (1_000, 1));
    assert!(dropped.results().is_empty());
    assert_eq!(h.core.next_request(s), Some(2));
    assert_eq!(h.server.applied(), 1);
}

/// A Drop whose reply is lost is sent again through its session, the same Drop with the same request, on the next
/// connection, and applied once (`PROTOCOL.md` §11.5).
#[test]
fn test_a_lost_drop_reply_is_retried_through_the_session_and_applied_once() {
    use ultragraphdb_protocol_wire::{BodyView, DropCommandView};
    let mut h = Harness::ready(server());
    let s = session(&mut h);
    h.server.drop_after_next_commit();
    let p = h
        .core
        .drop_command(
            ultragraphdb_protocol_wire::DropCommand::namespace_data(3),
            s,
            h.now,
        )
        .unwrap();
    let Ok(Reply::Dropped(dropped)) = h.settle(p) else {
        panic!("dropped through the retry")
    };
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
}

/// A drop is its session's commit in flight: the session takes no other until it is answered.
#[test]
fn test_a_drop_in_flight_keeps_its_session_busy() {
    let mut h = Harness::ready(server());
    let s = session(&mut h);
    let tx = begin(&mut h);
    let p = h
        .core
        .drop_command(ultragraphdb_protocol_wire::DropCommand::all(), s, h.now)
        .unwrap();
    assert_eq!(
        h.core.commit(tx, s, h.now).unwrap_err().0,
        ClientErrorEnum::SessionBusy { session: 9 }
    );
    assert_eq!(
        h.core
            .drop_command(ultragraphdb_protocol_wire::DropCommand::all(), s, h.now)
            .unwrap_err()
            .0,
        ClientErrorEnum::SessionBusy { session: 9 }
    );
    assert!(matches!(h.settle(p), Ok(Reply::Dropped(_))));
}

/// A Drop's reply naming another request than the one sent is no answer of it.
#[test]
fn test_a_drop_reply_of_another_request_is_a_mismatch() {
    use ultragraphdb_protocol_wire::{BodyBuilder, Command, DropCommand, FLAG_LAST, Header};
    let mut h = Harness::ready(server());
    let s = session(&mut h);
    let p = h.core.drop_command(DropCommand::all(), s, h.now).unwrap();
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
        )
        .unwrap();
    let reply = h.numbered(&reply);
    h.deliver(&reply).unwrap();
    assert_eq!(
        h.core.take_reply(p).unwrap().unwrap_err().0,
        ClientErrorEnum::ReplyMismatch {
            field: "request",
            expected: 1,
            found: 2
        }
    );
}
