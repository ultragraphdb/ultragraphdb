/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The registry asked by Session, Check and Ended (`PROTOCOL.md` §11.5, §13.23 to §13.25), the corner cases of
//! `notes/corner-cases.md` C3.1 to C3.49: the group run on the test's runtime over the simulated storage, by a clock the
//! test moves. `transaction_ms` is [`T_MS`], so a closed connection's marks and the quiet time last [`HOLD_US`].

use compio::runtime::Runtime;
use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_common_system::SystemHandle;
use ultragraphdb_protocol_wire::{CheckKind, CheckOutcome, ErrorCode, SessionCheck};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transactions_log::{FaultMode, SimStorage};

use super::{SimOwner, TestClock, ended, heard, principal, recovered, refused, running, starting};

/// `transaction_ms` of every test here.
pub(crate) const T_MS: u64 = 50;

/// Twice `transaction_ms` in microseconds: how long a closed connection's marks and the quiet time last
/// (`PROTOCOL.md` §11.5).
pub(crate) const HOLD_US: u64 = 2 * T_MS * 1_000;

/// The kind of a check that asks: a Recall's.
pub(crate) const ASKS: CheckKind = CheckKind::Asks;
/// The kind of a check that marks: a Commit's.
pub(crate) const MARKS: CheckKind = CheckKind::Marks;

/// A group over `storage` holding `alice` and `bob`, whose registry holds `sessions_max`, recovered at the clock's reading.
pub(crate) async fn group_of(
    storage: &SimStorage,
    clock: &TestClock,
    sessions_max: u32,
) -> TestResult<SimOwner> {
    let start =
        starting(MetadataConfig::new().with_sessions_max(sessions_max)?)?.with_transaction_ms(T_MS);
    let mut owner = recovered(storage, &start, clock).await?;
    super::with_principal(&mut owner, "alice").await?;
    super::with_principal(&mut owner, "bob").await?;
    Ok(owner)
}

/// The outcomes of one Check frame of `checks`, each a session, a principal and a kind, sent on `connection`, by number.
pub(crate) async fn outcomes(
    handle: &SystemHandle,
    connection: u64,
    checks: &[(u128, u64, CheckKind)],
) -> TestResult<Vec<u8>> {
    let sent: Vec<SessionCheck> = checks
        .iter()
        .map(|(session, principal, kind)| {
            SessionCheck::new(session.to_be_bytes(), *principal, *kind)
        })
        .collect();
    let answered = heard(handle.check(connection, sent.clone()).await)?;
    assert_eq!(answered.len(), sent.len(), "the frame answered whole");
    for (got, asked) in answered.iter().zip(&sent) {
        assert_eq!(
            (got.session(), got.principal(), got.kind()),
            (asked.session(), asked.principal(), asked.kind()),
            "every field as sent but the outcome"
        );
    }
    Ok(answered
        .iter()
        .map(|check| check.outcome().number())
        .collect())
}

/// The outcome of one check of `session` by `principal` of `kind` on `connection`.
pub(crate) async fn outcome(
    handle: &SystemHandle,
    connection: u64,
    session: u128,
    principal: u64,
    kind: CheckKind,
) -> TestResult<u8> {
    outcomes(handle, connection, &[(session, principal, kind)])
        .await?
        .first()
        .copied()
        .context("the outcome")
}

/// `connection` closed, the group having taken the close before the clock moves on: it holds the connection's marks from the
/// clock's reading at the close.
pub(crate) async fn disconnected(handle: &SystemHandle, connection: u64) -> TestResult {
    handle.disconnect(connection);
    let _latest = heard(handle.latest_close().await)?;
    Ok(())
}

/// A new connection's number.
pub(crate) async fn connected(handle: &SystemHandle) -> TestResult<u64> {
    handle.connect().await.context("a connection")
}

// The outcomes' numbers, from `PROTOCOL.md` §13.24.
const PASSED: u8 = 1;
const NOT_HELD: u8 = 12;
const IN_FLIGHT: u8 = 14;
const OTHER: u8 = 21;

/// The clock moved past the quiet time of a group that began to listen at its first reading.
fn after_quiet(clock: &TestClock) {
    clock.advance(HOLD_US);
}

/// Runs `body` over a group of `sessions_max` holding `alice` and `bob`, past its quiet time, handing it the handle, the
/// clock and the two principals; then ends the group.
fn with_group<F, Fut>(seed: u64, sessions_max: u32, quiet: bool, body: F) -> TestResult
where
    F: FnOnce(SystemHandle, TestClock, u64, u64) -> Fut,
    Fut: std::future::Future<Output = TestResult>,
{
    let clock = TestClock::new();
    let storage = SimStorage::new(seed);
    Runtime::new()?.block_on(async {
        let owner = group_of(&storage, &clock, sessions_max).await?;
        let (handle, run) = running(owner, &clock);
        let (alice, bob) = (
            principal(&handle, "alice").await?,
            principal(&handle, "bob").await?,
        );
        if !quiet {
            after_quiet(&clock);
        }
        body(handle, clock, alice, bob).await?;
        ended(run).await??;
        Ok(())
    })
}

/// C3.1, C3.2: an empty registry registers a Session, and one of `sessions_max` − 1 registers one more evicting none.
#[test]
fn test_a_registry_with_room_registers_and_evicts_none() -> TestResult {
    with_group(101, 3, false, |handle, _clock, alice, _bob| async move {
        heard(handle.session(1, alice).await)?;
        heard(handle.session(2, alice).await)?;
        heard(handle.session(3, alice).await)?;
        let connection = connected(&handle).await?;
        let checks = [(1, alice, ASKS), (2, alice, ASKS), (3, alice, ASKS)];
        assert_eq!(outcomes(&handle, connection, &checks).await?, [PASSED; 3]);
        Ok(())
    })
}

/// C3.3: a full registry, none marked, evicts its least recently registered session for a new one.
#[test]
fn test_a_full_registry_evicts_its_oldest_session() -> TestResult {
    with_group(102, 2, false, |handle, _clock, alice, _bob| async move {
        heard(handle.session(1, alice).await)?;
        heard(handle.session(2, alice).await)?;
        heard(handle.session(3, alice).await)?;
        let connection = connected(&handle).await?;
        let checks = [(1, alice, ASKS), (2, alice, ASKS), (3, alice, ASKS)];
        assert_eq!(
            outcomes(&handle, connection, &checks).await?,
            [NOT_HELD, PASSED, PASSED]
        );
        Ok(())
    })
}

/// C3.4, C3.6: a full registry whose oldest session is marked evicts the oldest without a mark; once an Ended takes the mark
/// away, that session is the one evicted.
#[test]
fn test_a_marked_session_is_not_evicted_until_its_mark_ends() -> TestResult {
    with_group(103, 2, false, |handle, _clock, alice, _bob| async move {
        heard(handle.session(1, alice).await)?;
        heard(handle.session(2, alice).await)?;
        let connection = connected(&handle).await?;
        assert_eq!(outcome(&handle, connection, 1, alice, MARKS).await?, PASSED);
        heard(handle.session(3, alice).await)?;
        let after = outcomes(&handle, connection, &[(1, alice, ASKS), (2, alice, ASKS)]).await?;
        assert_eq!(after, [IN_FLIGHT, NOT_HELD], "2 evicted, 1 marked and held");
        handle.ended(connection, vec![1]);
        heard(handle.session(4, alice).await)?;
        assert_eq!(
            outcome(&handle, connection, 1, alice, ASKS).await?,
            NOT_HELD
        );
        Ok(())
    })
}

/// C3.5, C3.6: a registry full of marked sessions refuses a Session with code 14, registering and evicting nothing; once an
/// Ended names one of them, that one is evicted for the new session.
#[test]
fn test_a_registry_full_of_marked_sessions_refuses_a_session() -> TestResult {
    with_group(104, 2, false, |handle, _clock, alice, _bob| async move {
        heard(handle.session(1, alice).await)?;
        heard(handle.session(2, alice).await)?;
        let connection = connected(&handle).await?;
        let marked = outcomes(&handle, connection, &[(1, alice, MARKS), (2, alice, MARKS)]).await?;
        assert_eq!(marked, [PASSED, PASSED]);
        let busy = refused(handle.session(3, alice).await)?;
        assert_eq!(busy.code(), ErrorCode::SessionBusy);
        let held = outcomes(
            &handle,
            connection,
            &[(1, alice, ASKS), (2, alice, ASKS), (3, alice, ASKS)],
        )
        .await?;
        assert_eq!(
            held,
            [IN_FLIGHT, IN_FLIGHT, NOT_HELD],
            "nothing registered, nothing evicted"
        );
        handle.ended(connection, vec![2]);
        heard(handle.session(3, alice).await)?;
        let after = outcomes(
            &handle,
            connection,
            &[(1, alice, ASKS), (2, alice, ASKS), (3, alice, ASKS)],
        )
        .await?;
        assert_eq!(after, [IN_FLIGHT, NOT_HELD, PASSED]);
        Ok(())
    })
}

/// C3.7, C3.8: a registry full of sessions whose marks' connections closed refuses a Session with code 14 until twice
/// `transaction_ms` after the close, and from that instant evicts the oldest.
#[test]
fn test_a_closed_connections_marks_hold_the_registry_for_twice_the_limit() -> TestResult {
    with_group(105, 2, false, |handle, clock, alice, _bob| async move {
        heard(handle.session(1, alice).await)?;
        heard(handle.session(2, alice).await)?;
        let connection = connected(&handle).await?;
        let marked = outcomes(&handle, connection, &[(1, alice, MARKS), (2, alice, MARKS)]).await?;
        assert_eq!(marked, [PASSED, PASSED]);
        disconnected(&handle, connection).await?;
        clock.advance(HOLD_US - 1);
        assert_eq!(
            refused(handle.session(3, alice).await)?.code(),
            ErrorCode::SessionBusy
        );
        clock.advance(1);
        heard(handle.session(3, alice).await)?;
        let other = connected(&handle).await?;
        let after = outcomes(
            &handle,
            other,
            &[(1, alice, ASKS), (2, alice, ASKS), (3, alice, ASKS)],
        )
        .await?;
        assert_eq!(after, [NOT_HELD, PASSED, PASSED]);
        Ok(())
    })
}

/// C3.9: a registry of one session evicts it unmarked, and refuses with code 14 while it is marked.
#[test]
fn test_a_registry_of_one_session() -> TestResult {
    with_group(106, 1, false, |handle, _clock, alice, _bob| async move {
        heard(handle.session(1, alice).await)?;
        heard(handle.session(2, alice).await)?;
        let connection = connected(&handle).await?;
        assert_eq!(
            outcome(&handle, connection, 1, alice, ASKS).await?,
            NOT_HELD
        );
        assert_eq!(outcome(&handle, connection, 2, alice, MARKS).await?, PASSED);
        assert_eq!(
            refused(handle.session(3, alice).await)?.code(),
            ErrorCode::SessionBusy
        );
        Ok(())
    })
}

/// C3.10, C3.11: a Session naming a session the registry holds is refused with Error 1 naming it, for its own principal and
/// another's, and the session stays its first principal's, held once.
#[test]
fn test_a_held_identity_is_refused_whoever_names_it() -> TestResult {
    with_group(107, 4, false, |handle, _clock, alice, bob| async move {
        let session: u128 = 0xab;
        heard(handle.session(session, alice).await)?;
        for again in [alice, bob] {
            let refusal = refused(handle.session(session, again).await)?;
            assert_eq!(refusal.code(), ErrorCode::Protocol, "{}", refusal.message());
            assert!(
                refusal.message().contains(&format!("{session:032x}")),
                "the message names the session: {}",
                refusal.message()
            );
        }
        let connection = connected(&handle).await?;
        let held = outcomes(&handle, connection, &[(session, alice, ASKS)]).await?;
        assert_eq!(held, [PASSED], "still alice's");
        assert_eq!(
            outcome(&handle, connection, session, bob, ASKS).await?,
            OTHER
        );
        Ok(())
    })
}

/// C3.12: a Session naming a principal the group does not hold is refused with code 7 naming it, registering nothing.
#[test]
fn test_an_unknown_principal_registers_nothing() -> TestResult {
    with_group(108, 4, false, |handle, _clock, alice, _bob| async move {
        let unknown = 977;
        let refusal = refused(handle.session(5, unknown).await)?;
        assert_eq!(refusal.code(), ErrorCode::Refused);
        assert!(refusal.message().contains("977"), "{}", refusal.message());
        let connection = connected(&handle).await?;
        assert_eq!(
            outcome(&handle, connection, 5, alice, ASKS).await?,
            NOT_HELD
        );
        Ok(())
    })
}

/// C3.13: a Session's refusals come in the order Error 1, code 7, code 14, and none evicts: a held session named for an unknown
/// principal is Error 1, at a registry full of marked sessions too, and a new one for an unknown principal there is code 7.
#[test]
fn test_a_sessions_refusals_follow_their_order() -> TestResult {
    with_group(109, 2, false, |handle, _clock, alice, _bob| async move {
        let unknown = 977;
        heard(handle.session(1, alice).await)?;
        assert_eq!(
            refused(handle.session(1, unknown).await)?.code(),
            ErrorCode::Protocol
        );
        heard(handle.session(2, alice).await)?;
        let connection = connected(&handle).await?;
        let marked = outcomes(&handle, connection, &[(1, alice, MARKS), (2, alice, MARKS)]).await?;
        assert_eq!(marked, [PASSED, PASSED]);
        assert_eq!(
            refused(handle.session(1, unknown).await)?.code(),
            ErrorCode::Protocol
        );
        assert_eq!(
            refused(handle.session(3, unknown).await)?.code(),
            ErrorCode::Refused
        );
        assert_eq!(
            refused(handle.session(3, alice).await)?.code(),
            ErrorCode::SessionBusy
        );
        let held = outcomes(&handle, connection, &[(1, alice, ASKS), (2, alice, ASKS)]).await?;
        assert_eq!(held, [IN_FLIGHT, IN_FLIGHT], "nothing evicted");
        Ok(())
    })
}

/// C3.14: a session evicted, its eviction carried by no close yet, is not held, so a Session naming it registers it again.
#[test]
fn test_an_evicted_identity_registers_again() -> TestResult {
    with_group(110, 1, false, |handle, _clock, alice, _bob| async move {
        heard(handle.session(1, alice).await)?;
        heard(handle.session(2, alice).await)?;
        heard(handle.session(1, alice).await)?;
        let connection = connected(&handle).await?;
        assert_eq!(
            outcomes(&handle, connection, &[(1, alice, ASKS), (2, alice, ASKS)]).await?,
            [PASSED, NOT_HELD]
        );
        Ok(())
    })
}

/// C3.15 to C3.20: a check's outcome is the first that applies of 12, 21, 14 and 1, whatever its kind; a passed check of kind 2
/// marks its session, one of kind 1 marks nothing.
#[test]
fn test_a_checks_outcome_follows_its_order() -> TestResult {
    with_group(111, 8, false, |handle, _clock, alice, bob| async move {
        let unknown = 977;
        heard(handle.session(1, alice).await)?;
        heard(handle.session(2, bob).await)?;
        heard(handle.session(3, bob).await)?;
        let connection = connected(&handle).await?;
        // C3.16: kind 1 passes and marks nothing, so a second kind 1 passes too.
        assert_eq!(
            outcomes(&handle, connection, &[(1, alice, ASKS)]).await?,
            [PASSED]
        );
        assert_eq!(
            outcomes(&handle, connection, &[(1, alice, ASKS)]).await?,
            [PASSED]
        );
        // C3.17, C3.19: a session never registered is 12, for either kind and an unknown principal alike.
        let never = outcomes(
            &handle,
            connection,
            &[(9, alice, ASKS), (10, alice, MARKS), (11, unknown, MARKS)],
        )
        .await?;
        assert_eq!(never, [NOT_HELD; 3]);
        // C3.18: another principal's session is 21 and marks nothing.
        assert_eq!(outcome(&handle, connection, 2, alice, MARKS).await?, OTHER);
        assert_eq!(outcome(&handle, connection, 2, bob, ASKS).await?, PASSED);
        // C3.15: kind 2 passes and marks.
        assert_eq!(outcome(&handle, connection, 3, bob, MARKS).await?, PASSED);
        assert_eq!(outcome(&handle, connection, 3, bob, ASKS).await?, IN_FLIGHT);
        // C3.20: another principal's marked session is 21, before 14.
        assert_eq!(outcome(&handle, connection, 3, alice, ASKS).await?, OTHER);
        Ok(())
    })
}

/// C3.21, C3.22: a marked session answers 14 to a check of kind 2 on its own connection and on another, and to one of kind 1 on
/// another, which marks nothing.
#[test]
fn test_a_marked_session_is_busy_on_every_connection() -> TestResult {
    with_group(112, 8, false, |handle, _clock, alice, _bob| async move {
        heard(handle.session(1, alice).await)?;
        let (first, second) = (connected(&handle).await?, connected(&handle).await?);
        assert_eq!(outcome(&handle, first, 1, alice, MARKS).await?, PASSED);
        assert_eq!(outcome(&handle, first, 1, alice, MARKS).await?, IN_FLIGHT);
        assert_eq!(outcome(&handle, second, 1, alice, MARKS).await?, IN_FLIGHT);
        assert_eq!(outcome(&handle, second, 1, alice, ASKS).await?, IN_FLIGHT);
        handle.ended(first, vec![1]);
        assert_eq!(
            outcome(&handle, second, 1, alice, MARKS).await?,
            PASSED,
            "the kind 1 check marked nothing"
        );
        Ok(())
    })
}

/// C3.23, C3.24: one frame of checks is decided in its order and answered whole: kind 2 of alice's, kind 2 of a session never
/// registered and kind 1 of bob's naming alice answer 1, 12 and 21, and alice's session alone is marked; a frame of one check
/// is answered whole too.
#[test]
fn test_a_frame_of_checks_is_answered_whole_and_in_order() -> TestResult {
    with_group(113, 8, false, |handle, _clock, alice, bob| async move {
        heard(handle.session(1, alice).await)?;
        heard(handle.session(3, bob).await)?;
        let connection = connected(&handle).await?;
        let frame = [(1, alice, MARKS), (2, alice, MARKS), (3, alice, ASKS)];
        assert_eq!(
            outcomes(&handle, connection, &frame).await?,
            [PASSED, NOT_HELD, OTHER]
        );
        let marked = outcomes(&handle, connection, &[(1, alice, ASKS), (3, bob, MARKS)]).await?;
        assert_eq!(
            marked,
            [IN_FLIGHT, PASSED],
            "alice's session alone was marked"
        );
        Ok(())
    })
}

/// A frame checking one session twice decides both in their order: a kind 2 check that passes marks the session the second
/// check finds in flight.
#[test]
fn test_a_second_check_of_a_session_in_one_frame_sees_the_first_mark() -> TestResult {
    with_group(114, 8, false, |handle, _clock, alice, _bob| async move {
        heard(handle.session(1, alice).await)?;
        let connection = connected(&handle).await?;
        let frame = [(1, alice, MARKS), (1, alice, MARKS)];
        assert_eq!(
            outcomes(&handle, connection, &frame).await?,
            [PASSED, IN_FLIGHT]
        );
        Ok(())
    })
}

/// C3.26, C3.27, C3.29, C3.30, C3.31: an Ended ends the marks begun on its own connection and passes over another open
/// connection's, a session without a mark and one never registered; sent again it changes nothing.
#[test]
fn test_an_ended_ends_its_own_connections_marks() -> TestResult {
    with_group(115, 8, false, |handle, _clock, alice, _bob| async move {
        for session in [1, 2, 3, 4] {
            heard(handle.session(session, alice).await)?;
        }
        let (first, second) = (connected(&handle).await?, connected(&handle).await?);
        assert_eq!(
            outcomes(&handle, first, &[(1, alice, MARKS), (2, alice, MARKS)]).await?,
            [PASSED; 2]
        );
        assert_eq!(outcome(&handle, second, 3, alice, MARKS).await?, PASSED);
        // C3.27: the other open connection's Ended leaves 1 marked.
        handle.ended(second, vec![1]);
        assert_eq!(outcome(&handle, second, 1, alice, ASKS).await?, IN_FLIGHT);
        // C3.31, C3.30: the first connection's Ended of 1, 2, 3, an unmarked 4 and an unregistered 9 ends 1 and 2 alone.
        handle.ended(first, vec![1, 2, 3, 4, 9]);
        let after = outcomes(
            &handle,
            second,
            &[
                (1, alice, ASKS),
                (2, alice, ASKS),
                (3, alice, ASKS),
                (4, alice, ASKS),
            ],
        )
        .await?;
        assert_eq!(after, [PASSED, PASSED, IN_FLIGHT, PASSED]);
        // C3.29: the same Ended again changes nothing.
        handle.ended(first, vec![1, 2, 3, 4, 9]);
        assert_eq!(outcome(&handle, second, 3, alice, ASKS).await?, IN_FLIGHT);
        Ok(())
    })
}

/// C3.28: once a mark's connection closed, an Ended on any connection ends it, and a check of kind 2 then passes.
#[test]
fn test_an_orphaned_mark_is_ended_from_another_connection() -> TestResult {
    with_group(116, 8, false, |handle, _clock, alice, _bob| async move {
        heard(handle.session(1, alice).await)?;
        let (first, second) = (connected(&handle).await?, connected(&handle).await?);
        assert_eq!(outcome(&handle, first, 1, alice, MARKS).await?, PASSED);
        disconnected(&handle, first).await?;
        assert_eq!(outcome(&handle, second, 1, alice, MARKS).await?, IN_FLIGHT);
        handle.ended(second, vec![1]);
        assert_eq!(outcome(&handle, second, 1, alice, MARKS).await?, PASSED);
        Ok(())
    })
}

/// C3.33, C3.34, C3.35, C3.38: a closed connection's mark holds at every time before twice `transaction_ms` after the close and
/// is gone from that instant, whatever was asked meanwhile; then a check of kind 2 passes and marks against its own connection.
#[test]
fn test_a_closed_connections_mark_ends_at_twice_the_limit() -> TestResult {
    with_group(117, 8, false, |handle, clock, alice, _bob| async move {
        heard(handle.session(1, alice).await)?;
        heard(handle.session(2, alice).await)?;
        let (first, second) = (connected(&handle).await?, connected(&handle).await?);
        assert_eq!(
            outcomes(&handle, first, &[(1, alice, MARKS), (2, alice, MARKS)]).await?,
            [PASSED; 2]
        );
        disconnected(&handle, first).await?;
        clock.advance(HOLD_US - 1);
        assert_eq!(outcome(&handle, second, 1, alice, ASKS).await?, IN_FLIGHT);
        assert_eq!(outcome(&handle, second, 2, alice, MARKS).await?, IN_FLIGHT);
        clock.advance(1);
        assert_eq!(outcome(&handle, second, 1, alice, ASKS).await?, PASSED);
        assert_eq!(outcome(&handle, second, 2, alice, MARKS).await?, PASSED);
        // The new mark is held against `second`, which stays open: no time ends it.
        clock.advance(HOLD_US * 3);
        assert_eq!(outcome(&handle, first, 2, alice, ASKS).await?, IN_FLIGHT);
        Ok(())
    })
}

/// C3.36, C3.37: marks of two connections closed `transaction_ms` apart end each at its own time; a connection closing with no
/// mark changes nothing.
#[test]
fn test_two_closed_connections_marks_end_each_at_its_time() -> TestResult {
    with_group(118, 8, false, |handle, clock, alice, _bob| async move {
        heard(handle.session(1, alice).await)?;
        heard(handle.session(2, alice).await)?;
        heard(handle.session(3, alice).await)?;
        let (first, second, third, asking) = (
            connected(&handle).await?,
            connected(&handle).await?,
            connected(&handle).await?,
            connected(&handle).await?,
        );
        assert_eq!(outcome(&handle, first, 1, alice, MARKS).await?, PASSED);
        assert_eq!(outcome(&handle, second, 2, alice, MARKS).await?, PASSED);
        disconnected(&handle, third).await?;
        disconnected(&handle, first).await?;
        clock.advance(T_MS * 1_000);
        disconnected(&handle, second).await?;
        clock.advance(T_MS * 1_000);
        let at_first = outcomes(
            &handle,
            asking,
            &[(1, alice, ASKS), (2, alice, ASKS), (3, alice, ASKS)],
        )
        .await?;
        assert_eq!(at_first, [PASSED, IN_FLIGHT, PASSED]);
        clock.advance(T_MS * 1_000);
        assert_eq!(outcome(&handle, asking, 2, alice, ASKS).await?, PASSED);
        Ok(())
    })
}

/// C3.39: a full registry whose oldest session is marked on a connection closed at `t` evicts the next oldest for a Session one
/// tick before `t + 2T`, and the oldest at `t + 2T`.
#[test]
fn test_a_full_registry_evicts_a_closed_connections_session_at_twice_the_limit() -> TestResult {
    with_group(119, 2, false, |handle, clock, alice, _bob| async move {
        heard(handle.session(1, alice).await)?;
        heard(handle.session(2, alice).await)?;
        let (first, asking) = (connected(&handle).await?, connected(&handle).await?);
        assert_eq!(outcome(&handle, first, 1, alice, MARKS).await?, PASSED);
        disconnected(&handle, first).await?;
        clock.advance(HOLD_US - 1);
        heard(handle.session(3, alice).await)?;
        assert_eq!(
            outcomes(&handle, asking, &[(1, alice, ASKS), (2, alice, ASKS)]).await?,
            [IN_FLIGHT, NOT_HELD]
        );
        clock.advance(1);
        heard(handle.session(4, alice).await)?;
        assert_eq!(
            outcomes(&handle, asking, &[(1, alice, ASKS), (3, alice, ASKS)]).await?,
            [NOT_HELD, PASSED]
        );
        Ok(())
    })
}

/// C3.40 to C3.42, C3.45 to C3.47, C3.49: within twice `transaction_ms` of the instant the group begins to listen, a check of
/// kind 1 that would pass answers 14, one naming a session not held 12 and another principal's 21, and one of kind 2 passes and
/// marks; at exactly twice `transaction_ms` the quiet time has ended. The cluster's first start is quiet as any start is.
#[test]
fn test_the_quiet_time_answers_no_resend() -> TestResult {
    with_group(120, 8, true, |handle, clock, alice, bob| async move {
        heard(handle.session(1, alice).await)?;
        heard(handle.session(2, alice).await)?;
        heard(handle.session(3, bob).await)?;
        let connection = connected(&handle).await?;
        let quiet = outcomes(
            &handle,
            connection,
            &[(1, alice, ASKS), (3, alice, ASKS), (9, alice, ASKS)],
        )
        .await?;
        assert_eq!(quiet, [IN_FLIGHT, OTHER, NOT_HELD]);
        assert_eq!(outcome(&handle, connection, 2, alice, MARKS).await?, PASSED);
        assert_eq!(
            outcome(&handle, connection, 2, alice, ASKS).await?,
            IN_FLIGHT
        );
        clock.advance(HOLD_US - 1);
        assert_eq!(
            outcome(&handle, connection, 1, alice, ASKS).await?,
            IN_FLIGHT
        );
        clock.advance(1);
        assert_eq!(outcome(&handle, connection, 1, alice, ASKS).await?, PASSED);
        clock.advance(1);
        assert_eq!(outcome(&handle, connection, 3, bob, ASKS).await?, PASSED);
        Ok(())
    })
}

/// C3.43 to C3.47: within the quiet time a Session that would evict is refused with code 14, evicting nothing, and one finding
/// room registers; at exactly twice `transaction_ms` it evicts the oldest.
#[test]
fn test_the_quiet_time_evicts_no_session() -> TestResult {
    with_group(121, 2, true, |handle, clock, alice, _bob| async move {
        heard(handle.session(1, alice).await)?;
        heard(handle.session(2, alice).await)?;
        assert_eq!(
            refused(handle.session(3, alice).await)?.code(),
            ErrorCode::SessionBusy
        );
        clock.advance(HOLD_US - 1);
        assert_eq!(
            refused(handle.session(3, alice).await)?.code(),
            ErrorCode::SessionBusy
        );
        clock.advance(1);
        heard(handle.session(3, alice).await)?;
        let connection = connected(&handle).await?;
        let after = outcomes(
            &handle,
            connection,
            &[(1, alice, ASKS), (2, alice, ASKS), (3, alice, ASKS)],
        )
        .await?;
        assert_eq!(after, [NOT_HELD, PASSED, PASSED]);
        clock.advance(1);
        heard(handle.session(4, alice).await)?;
        Ok(())
    })
}

/// C3.48, and the scenario "A restarted group answers no resend at once": a restart keeps the sessions registered and loses
/// every mark, and its quiet time begins again where it begins to listen.
#[test]
fn test_a_restart_keeps_the_sessions_and_loses_the_marks() -> TestResult {
    let clock = TestClock::new();
    let storage = SimStorage::new(122);
    Runtime::new()?.block_on(async {
        let owner = group_of(&storage, &clock, 2).await?;
        let (handle, run) = running(owner, &clock);
        let alice = principal(&handle, "alice").await?;
        after_quiet(&clock);
        heard(handle.session(1, alice).await)?;
        heard(handle.session(2, alice).await)?;
        let connection = connected(&handle).await?;
        assert_eq!(outcome(&handle, connection, 1, alice, MARKS).await?, PASSED);
        drop(handle);
        ended(run).await??;
        let restarted = recovered(
            &storage,
            &starting(MetadataConfig::new().with_sessions_max(2)?)?.with_transaction_ms(T_MS),
            &clock,
        )
        .await?;
        let (handle, run) = running(restarted, &clock);
        let connection = connected(&handle).await?;
        let quiet = outcomes(&handle, connection, &[(1, alice, ASKS), (2, alice, ASKS)]).await?;
        assert_eq!(quiet, [IN_FLIGHT, IN_FLIGHT], "held, and quiet");
        assert_eq!(
            outcome(&handle, connection, 1, alice, MARKS).await?,
            PASSED,
            "the mark is gone"
        );
        assert_eq!(
            refused(handle.session(3, alice).await)?.code(),
            ErrorCode::SessionBusy
        );
        after_quiet(&clock);
        assert_eq!(outcome(&handle, connection, 2, alice, ASKS).await?, PASSED);
        drop(handle);
        ended(run).await??;
        Ok(())
    })
}

/// The shrunk counterexample of `test_the_registry_agrees_with_its_model`: a registry of 1 holding alice's session 1
/// restarts, twice `transaction_ms` passes before the restarted group's run takes its first turn, and alice's Session 2 is
/// refused with code 14. The quiet time counts from the instant the run begins (`notes/group-3.md`, *Found wanting* 2), which
/// is that Session's turn; twice `transaction_ms` after it, Session 2 evicts session 1.
#[test]
fn test_session_2_after_a_restart_is_quiet_from_the_runs_first_turn() -> TestResult {
    let clock = TestClock::new();
    let storage = SimStorage::new(124);
    Runtime::new()?.block_on(async {
        let owner = group_of(&storage, &clock, 1).await?;
        let (handle, run) = running(owner, &clock);
        let alice = principal(&handle, "alice").await?;
        heard(handle.session(1, alice).await)?;
        drop(handle);
        ended(run).await??;
        let restarted = recovered(
            &storage,
            &starting(MetadataConfig::new().with_sessions_max(1)?)?.with_transaction_ms(T_MS),
            &clock,
        )
        .await?;
        let (handle, run) = running(restarted, &clock);
        clock.advance(HOLD_US);
        assert_eq!(
            refused(handle.session(2, alice).await)?.code(),
            ErrorCode::SessionBusy
        );
        clock.advance(HOLD_US - 1);
        assert_eq!(
            refused(handle.session(2, alice).await)?.code(),
            ErrorCode::SessionBusy
        );
        clock.advance(1);
        heard(handle.session(2, alice).await)?;
        let connection = connected(&handle).await?;
        let after = outcomes(&handle, connection, &[(1, alice, ASKS), (2, alice, ASKS)]).await?;
        assert_eq!(after, [NOT_HELD, PASSED]);
        drop(handle);
        ended(run).await??;
        Ok(())
    })
}

/// The handle of a group that stopped answers a Check as unavailable, and gives no connection.
#[test]
fn test_a_stopped_group_answers_no_check() -> TestResult {
    let clock = TestClock::new();
    let storage = SimStorage::new(123);
    Runtime::new()?.block_on(async {
        let owner = group_of(&storage, &clock, 2).await?;
        let (handle, run) = running(owner, &clock);
        let alice = principal(&handle, "alice").await?;
        // The next operation is the write of the next close's round, which stops the group.
        storage.fail_at(storage.trace().len(), FaultMode::Before);
        clock.advance(super::EPOCH_US);
        assert!(ended(run).await?.is_err(), "the group stopped");
        assert!(handle.connect().await.is_none(), "no connection");
        let check = vec![SessionCheck::new(1u128.to_be_bytes(), alice, ASKS)];
        let refusal = handle.check(1, check).await.err().context("a refusal")?;
        assert_eq!(refusal.code(), ErrorCode::Unavailable);
        Ok(())
    })
}

/// The outcomes a check names are those of `PROTOCOL.md` §13.24, by number.
#[test]
fn test_the_outcomes_numbers_are_the_protocols() {
    assert_eq!(
        [
            CheckOutcome::Passed,
            CheckOutcome::NotHeld,
            CheckOutcome::InFlight,
            CheckOutcome::OtherPrincipal
        ]
        .map(CheckOutcome::number),
        [PASSED, NOT_HELD, IN_FLIGHT, OTHER]
    );
}
