/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A turn's frames: corner cases C5.19 to C5.27 of `notes/corner-cases.md` (`PROTOCOL.md` §11.5, *Turns*).

use ultragraphdb_common_compute::SystemTurns;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Check, CheckKind, CheckView, Command, Ended, EndedView, FrameView,
    Header, MAX_FRAME_FLOOR_U32, MAX_FRAME_SIZE, SessionCheck,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

const CLUSTER: u64 = 7;
const ALICE: u64 = 11;

/// The session numbered `n`, big-endian as the wire holds it.
fn session(n: u128) -> [u8; 16] {
    n.to_be_bytes()
}

fn header(command: Command) -> Header {
    Header::new(CLUSTER, command).with_stream(3)
}

/// The largest count of entries whose frame `build` makes within `max_frame`, found by bisecting over built frames: an oracle
/// by search, apart from the code's arithmetic.
fn fits(max_frame: u32, build: impl Fn(usize) -> Option<usize>) -> usize {
    let within = |count: usize| build(count).is_some_and(|len| len <= max_frame as usize);
    let (mut low, mut high) = (1usize, max_frame as usize);
    while low < high {
        let middle = low + (high - low).div_ceil(2);
        if within(middle) {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    low
}

fn check_len(count: usize) -> Option<usize> {
    let checks = (0..count)
        .map(|n| SessionCheck::new(session(n as u128 + 1), ALICE, CheckKind::Marks))
        .collect();
    Check::new(checks)
        .frame(header(Command::Check))
        .ok()
        .map(|f| f.len())
}

fn ended_len(count: usize) -> Option<usize> {
    let sessions = (0..count).map(|n| session(n as u128 + 1)).collect();
    Ended::new(sessions)
        .frame(header(Command::Ended))
        .ok()
        .map(|f| f.len())
}

/// The sessions of a Check frame, read by the wire's view.
fn checked(frame: &[u8]) -> TestResult<Vec<SessionCheck>> {
    let view = FrameView::validate(frame, CLUSTER, MAX_FRAME_SIZE)?;
    let checks = CheckView::from_frame(&view)?;
    Ok((0..checks.len()).filter_map(|at| checks.get(at)).collect())
}

/// C5.19: nothing queued sends no frame.
#[test]
fn test_a_turn_of_nothing_sends_no_frame() -> TestResult {
    let mut turns = SystemTurns::new();
    assert!(turns.is_empty());
    let turn = turns.take(8, 8);
    assert!(turn.is_empty());
    assert_eq!(turn.check_frame(header(Command::Check)), None);
    assert_eq!(turn.ended_frame(header(Command::Ended)), None);
    Ok(())
}

/// C5.20: one Check queued is one Check frame of one check, as sent.
#[test]
fn test_one_check_is_one_frame_of_one_check() -> TestResult {
    let mut turns = SystemTurns::new();
    let ticket = turns.queue_check(session(5), ALICE, CheckKind::Marks);
    let turn = turns.take(8, 8);
    assert_eq!(turn.checks().len(), 1);
    assert_eq!(turn.checks().first().map(|(t, _)| *t), Some(ticket));
    let frame = turn
        .check_frame(header(Command::Check))
        .context("a Check frame")?;
    assert_eq!(
        checked(&frame)?,
        vec![SessionCheck::new(session(5), ALICE, CheckKind::Marks)]
    );
    assert!(turns.is_empty(), "nothing waits");
    Ok(())
}

/// C5.21: eight Commits of eight sessions queued out of order leave in one Check frame, sessions ascending.
#[test]
fn test_eight_sessions_share_one_frame_ascending() -> TestResult {
    let mut turns = SystemTurns::new();
    let order = [8u128, 3, 6, 1, 7, 2, 5, 4];
    for n in order {
        let _ticket = turns.queue_check(session(n), ALICE, CheckKind::Marks);
    }
    let turn = turns.take(16, 16);
    let frame = turn
        .check_frame(header(Command::Check))
        .context("a Check frame")?;
    let sessions: Vec<[u8; 16]> = checked(&frame)?.iter().map(SessionCheck::session).collect();
    assert_eq!(sessions, (1..=8).map(session).collect::<Vec<_>>());
    Ok(())
}

/// C5.22, C5.26: a Commit and a resend of one session take one check each turn, the second in the next, unanswered first.
#[test]
fn test_a_sessions_second_check_waits_for_the_next_turn() -> TestResult {
    let mut turns = SystemTurns::new();
    let first = turns.queue_check(session(4), ALICE, CheckKind::Marks);
    let second = turns.queue_check(session(4), ALICE, CheckKind::Asks);
    let turn = turns.take(16, 16);
    assert_eq!(
        turn.checks().iter().map(|(t, _)| *t).collect::<Vec<_>>(),
        vec![first]
    );
    assert!(!turns.is_empty(), "the resend waits");
    let next = turns.take(16, 16);
    assert_eq!(
        next.checks()
            .iter()
            .map(|(t, c)| (*t, c.kind()))
            .collect::<Vec<_>>(),
        vec![(second, CheckKind::Asks)]
    );
    Ok(())
}

/// C5.23, C5.24: an Ended and a Check of one session are one turn's two frames; two Endeds of a session are one.
#[test]
fn test_endeds_name_each_session_once() -> TestResult {
    let mut turns = SystemTurns::new();
    turns.queue_ended(session(9));
    turns.queue_ended(session(2));
    turns.queue_ended(session(9));
    let _ticket = turns.queue_check(session(9), ALICE, CheckKind::Marks);
    let turn = turns.take(16, 16);
    assert_eq!(turn.ended(), &[session(2), session(9)]);
    assert_eq!(turn.checks().len(), 1);
    let frame = turn
        .ended_frame(header(Command::Ended))
        .context("an Ended frame")?;
    let view = FrameView::validate(&frame, CLUSTER, MAX_FRAME_SIZE)?;
    assert_eq!(
        EndedView::from_frame(&view)?.sessions(),
        &[session(2), session(9)]
    );
    Ok(())
}

/// C5.25: what is queued after a turn leaves in the next.
#[test]
fn test_a_check_queued_after_a_turn_leaves_in_the_next() -> TestResult {
    let mut turns = SystemTurns::new();
    let _first = turns.queue_check(session(1), ALICE, CheckKind::Marks);
    let turn = turns.take(16, 16);
    let later = turns.queue_check(session(2), ALICE, CheckKind::Marks);
    assert_eq!(turn.checks().len(), 1);
    let next = turns.take(16, 16);
    assert_eq!(next.checks().first().map(|(t, _)| *t), Some(later));
    Ok(())
}

/// C5.27: more checks than a frame holds leave as many as fit, the lowest sessions, the rest in the next turn.
#[test]
fn test_checks_past_a_frame_wait_for_the_next_turn() -> TestResult {
    let mut turns = SystemTurns::new();
    for n in (1..=5u128).rev() {
        let _ticket = turns.queue_check(session(n), ALICE, CheckKind::Marks);
    }
    let turn = turns.take(3, 16);
    let sessions: Vec<[u8; 16]> = turn.checks().iter().map(|(_, c)| c.session()).collect();
    assert_eq!(sessions, vec![session(1), session(2), session(3)]);
    let next = turns.take(3, 16);
    let rest: Vec<[u8; 16]> = next.checks().iter().map(|(_, c)| c.session()).collect();
    assert_eq!(rest, vec![session(4), session(5)]);
    Ok(())
}

/// The counts a frame holds are the largest whose frame fits `max_frame`, at the floor and at a frame of one entry.
#[test]
fn test_a_frame_holds_what_max_frame_takes() -> TestResult {
    for max_frame in [MAX_FRAME_FLOOR_U32, 4_096, 1 << 20] {
        assert_eq!(
            SystemTurns::checks_max(max_frame),
            fits(max_frame, check_len)
        );
        assert_eq!(
            SystemTurns::ended_max(max_frame),
            fits(max_frame, ended_len)
        );
    }
    let one = u32::try_from(check_len(1).context("a frame of one check")?)?;
    assert_eq!(SystemTurns::checks_max(one), 1);
    Ok(())
}

/// A check taken back before its turn never leaves.
#[test]
fn test_a_withdrawn_check_never_leaves() -> TestResult {
    let mut turns = SystemTurns::new();
    let ticket = turns.queue_check(session(1), ALICE, CheckKind::Marks);
    assert!(turns.withdraw(ticket));
    assert!(!turns.withdraw(ticket));
    assert!(turns.take(16, 16).is_empty());
    Ok(())
}
