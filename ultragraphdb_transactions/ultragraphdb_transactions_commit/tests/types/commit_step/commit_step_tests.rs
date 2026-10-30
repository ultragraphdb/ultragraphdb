/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, ConflictKind, ErrorCode, Header, RoundBody,
};
use ultragraphdb_traits_log::Position;
use ultragraphdb_transactions_commit::{
    CommitError, CommitRequest, CommitStep, Input, Reply, RoundBounds, RoundRecord, StepOutput,
};
use ultragraphdb_transactions_log::WalConfig;
use ultragraphdb_transactions_txn::ReadSet;

use crate::utils::proposal_double::{Reads, TestProposal, Writes, proposal};

type Step = CommitStep<TestProposal>;
type Out = StepOutput<TestProposal>;

fn unbounded() -> RoundBounds {
    RoundBounds::new(u32::MAX, u32::MAX)
}

fn blind(key: &[u8]) -> TestProposal {
    proposal(0, &Reads::default(), &Writes::point(key))
}

fn commit(step: &mut Step, out: &mut Out, ticket: u64, p: TestProposal, now_us: u64) {
    step.step(Input::Commit(CommitRequest::new(ticket, p)), now_us, out);
}

fn sync(step: &mut Step, out: &mut Out, now_us: u64) {
    step.step(Input::Synced, now_us, out);
}

/// The versions of the apply, in order.
fn applied_versions(out: &Out) -> Vec<u64> {
    out.apply().iter().map(|(v, _)| *v).collect()
}

/// The frame size of a round holding `records`, committed, by the wire's own builder.
fn frame_of(records: &[&TestProposal]) -> usize {
    let mut body = RoundBody::new(0, 0, 0, 2);
    for p in records {
        body.commit(p.record.clone());
    }
    body.frame(Header::new(0, Command::Round))
        .expect("within the frame")
        .len()
}

#[test]
fn test_a_lone_commit_on_an_idle_step_closes_a_round_at_once() {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    let p = blind(b"k");
    let record = p.record.clone();
    commit(&mut step, &mut out, 1, p, 0);
    let (round, body) = out.append().expect("a round closed");
    // max(0 + 2, 2 × 0) = 2.
    assert_eq!(round, RoundRecord::new(2, 1, 1));
    let mut expected = RoundBody::new(0, 0, 0, 2);
    expected.commit(record);
    assert_eq!(body, expected.body().as_slice());
    assert!(out.replies().is_empty());
    assert!(out.apply().is_empty());
    assert_eq!(out.applied(), None);
    assert_eq!(step.in_flight(), Some(RoundRecord::new(2, 1, 1)));
    assert_eq!(step.in_flight_admitted().len(), 1);
    assert_eq!(step.in_flight_admitted()[0].0, 2);
    assert_eq!(step.watermark(), 0);
    // Acknowledged after its sync, with its apply.
    sync(&mut step, &mut out, 0);
    assert_eq!(out.applied(), Some(RoundRecord::new(2, 1, 1)));
    assert_eq!(applied_versions(&out), vec![2]);
    assert_eq!(out.replies(), &[Reply::new(1, Ok(2))]);
    assert_eq!(out.append(), None);
    assert_eq!(step.in_flight(), None);
    assert_eq!(step.watermark(), 2);
    assert_eq!(step.previous_version(), 2);
}

#[test]
fn test_commits_during_a_sync_wait_and_share_the_next_round() {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    commit(&mut step, &mut out, 0, blind(b"first"), 0);
    for ticket in 1..=10u64 {
        commit(&mut step, &mut out, ticket, blind(&ticket.to_be_bytes()), 0);
        assert_eq!(out.append(), None);
        assert!(out.replies().is_empty());
    }
    assert_eq!(step.waiting(), 10);
    sync(&mut step, &mut out, 0);
    assert_eq!(out.replies(), &[Reply::new(0, Ok(2))]);
    let (round, _) = out.append().expect("the next round closed");
    // Versions 4, 6, …, 22.
    assert_eq!(round, RoundRecord::new(4, 10, 10));
    assert_eq!(step.waiting(), 0);
    sync(&mut step, &mut out, 0);
    let expected: Vec<Reply> = (1..=10u64).map(|t| Reply::new(t, Ok(2 + 2 * t))).collect();
    assert_eq!(out.replies(), expected.as_slice());
    assert_eq!(
        applied_versions(&out),
        (4..=22).step_by(2).collect::<Vec<u64>>()
    );
    assert_eq!(step.watermark(), 22);
}

#[test]
fn test_a_round_holds_at_most_the_bound_s_transactions() {
    let mut step = Step::new(RoundBounds::new(u32::MAX, 3), 0);
    let mut out = Out::new();
    commit(&mut step, &mut out, 0, blind(b"first"), 0);
    for ticket in 1..=7u64 {
        commit(&mut step, &mut out, ticket, blind(&ticket.to_be_bytes()), 0);
    }
    sync(&mut step, &mut out, 0);
    assert_eq!(out.append().map(|(r, _)| r.transactions()), Some(3));
    assert_eq!(step.waiting(), 4);
    sync(&mut step, &mut out, 0);
    assert_eq!(out.append().map(|(r, _)| r.transactions()), Some(3));
    sync(&mut step, &mut out, 0);
    assert_eq!(out.append().map(|(r, _)| r.transactions()), Some(1));
    assert_eq!(step.waiting(), 0);
}

#[test]
fn test_a_round_holds_what_fits_its_bytes() {
    let ps: Vec<TestProposal> = (0..4u8)
        .map(|i| {
            let mut p = blind(&[i]);
            p.record = crate::utils::proposal_double::record(0, 0, &Writes::point(&[i]), 5_000);
            p
        })
        .collect();
    let two = frame_of(&[&ps[0], &ps[1]]) as u32;
    for (bound, held) in [(two, 2u32), (two - 1, 1)] {
        let mut step = Step::new(RoundBounds::new(bound, u32::MAX), 0);
        let mut out = Out::new();
        commit(&mut step, &mut out, 9, blind(b"first"), 0);
        for (t, p) in ps.iter().enumerate() {
            commit(&mut step, &mut out, t as u64, p.clone(), 0);
        }
        sync(&mut step, &mut out, 0);
        assert_eq!(
            out.append().map(|(r, _)| r.transactions()),
            Some(held),
            "bound {bound}"
        );
    }
}

#[test]
fn test_a_request_too_large_for_any_round_is_refused_when_it_arrives() {
    let mut p = blind(b"big");
    p.record = crate::utils::proposal_double::record(0, 0, &Writes::point(b"big"), 9_000);
    let alone = frame_of(&[&p]);
    let bound = alone as u32 - 1;
    let size = p.record.len();
    let mut step = Step::new(RoundBounds::new(bound, u32::MAX), 0);
    let mut out = Out::new();
    commit(&mut step, &mut out, 5, p, 0);
    // The record's size against the largest record a round of it alone holds, the wire's builder's measure.
    assert_eq!(
        out.replies(),
        &[Reply::new(
            5,
            Err(CommitError::TooLarge(
                size,
                crate::utils::frames::largest_record(bound as usize)
            ))
        )]
    );
    assert_eq!(out.append(), None);
    assert_eq!(step.waiting(), 0);
    assert_eq!(step.in_flight(), None);
}

#[test]
fn test_a_request_whose_round_alone_is_exactly_the_bound_is_taken() {
    // The bound is inclusive: a round of exactly `bytes` fits (the request above is refused one byte below it).
    let mut p = blind(b"big");
    p.record = crate::utils::proposal_double::record(0, 0, &Writes::point(b"big"), 9_000);
    let alone = frame_of(&[&p]);
    let mut step = Step::new(RoundBounds::new(alone as u32, u32::MAX), 0);
    let mut out = Out::new();
    commit(&mut step, &mut out, 5, p, 0);
    assert!(out.replies().is_empty(), "{:?}", out.replies());
    assert_eq!(
        out.append().map(|(r, _)| r),
        Some(RoundRecord::new(2, 1, 1))
    );
    sync(&mut step, &mut out, 0);
    assert_eq!(out.replies(), &[Reply::new(5, Ok(2))]);
}

#[test]
fn test_a_round_s_body_is_the_wire_s_round_body() {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    commit(&mut step, &mut out, 0, blind(b"first"), 0);
    let a = proposal(0, &Reads::default(), &Writes::point(b"k"));
    let b = proposal(0, &Reads::list(b"k"), &Writes::point(b"b"));
    let c = proposal(0, &Reads::list(b"b"), &Writes::point(b"c"));
    let (ra, rc) = (a.record.clone(), c.record.clone());
    commit(&mut step, &mut out, 1, a, 0);
    commit(&mut step, &mut out, 2, b, 0);
    commit(&mut step, &mut out, 3, c, 0);
    sync(&mut step, &mut out, 0);
    let (round, body) = out.append().expect("the round closed");
    // A at 4; B conflicts with A's write and takes no version; C's read of what B would have written does not
    // conflict, so it takes 6.
    assert_eq!(round, RoundRecord::new(4, 3, 2));
    let mut expected = RoundBody::new(0, 0, 0, 4);
    expected.commit(ra);
    expected.abort(ErrorCode::Conflict, ConflictKind::List.number());
    expected.commit(rc);
    assert_eq!(body, expected.body().as_slice());
    let r = out
        .round_ref(Position::new(2), 77)
        .expect("a round to append");
    assert_eq!(r.position(), Position::new(2));
    assert_eq!(r.parent(), 77);
    assert_eq!(r.first_version(), 4);
    assert_eq!(r.transactions(), 3);
    assert_eq!(r.body(), expected.body().as_slice());
    sync(&mut step, &mut out, 0);
    assert_eq!(
        out.replies(),
        &[
            Reply::new(1, Ok(4)),
            Reply::new(2, Err(CommitError::Conflict(ConflictKind::List))),
            Reply::new(3, Ok(6)),
        ]
    );
    assert_eq!(applied_versions(&out), vec![4, 6]);
}

#[test]
fn test_a_round_of_aborts_alone_keeps_the_watermark() {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    commit(&mut step, &mut out, 0, blind(b"k"), 0);
    commit(
        &mut step,
        &mut out,
        1,
        proposal(0, &Reads::list(b"k"), &Writes::point(b"x")),
        0,
    );
    sync(&mut step, &mut out, 0);
    assert_eq!(
        out.append().map(|(r, _)| r),
        Some(RoundRecord::new(4, 1, 0))
    );
    sync(&mut step, &mut out, 0);
    assert_eq!(out.applied(), Some(RoundRecord::new(4, 1, 0)));
    assert!(out.apply().is_empty());
    assert_eq!(step.watermark(), 2);
    // The next commit follows 2.
    commit(&mut step, &mut out, 2, blind(b"y"), 0);
    assert_eq!(out.append().map(|(r, _)| r.first_version()), Some(4));
}

#[test]
fn test_a_sync_with_no_round_in_flight_does_nothing() {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    sync(&mut step, &mut out, 0);
    assert_eq!(out.append(), None);
    assert_eq!(out.applied(), None);
    assert!(out.replies().is_empty());
    assert!(!step.is_stopped());
}

#[test]
fn test_a_failed_sync_acknowledges_nothing_and_stops_the_step() {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    commit(&mut step, &mut out, 1, blind(b"a"), 0);
    commit(&mut step, &mut out, 2, blind(b"b"), 0);
    commit(&mut step, &mut out, 3, blind(b"c"), 0);
    step.step(Input::SyncFailed, 0, &mut out);
    let unavailable: Vec<Reply> = (1..=3)
        .map(|t| Reply::new(t, Err(CommitError::Unavailable())))
        .collect();
    assert_eq!(out.replies(), unavailable.as_slice());
    assert_eq!(out.applied(), None);
    assert!(out.apply().is_empty());
    assert_eq!(out.append(), None);
    assert!(step.is_stopped());
    assert_eq!(step.in_flight(), None);
    assert_eq!(step.waiting(), 0);
    assert_eq!(step.watermark(), 0);
    commit(&mut step, &mut out, 4, blind(b"d"), 0);
    assert_eq!(
        out.replies(),
        &[Reply::new(4, Err(CommitError::Unavailable()))]
    );
    assert_eq!(out.append(), None);
    sync(&mut step, &mut out, 0);
    assert!(out.replies().is_empty() && out.append().is_none() && out.applied().is_none());
}

#[test]
fn test_the_oldest_read_prunes_and_older_reads_are_too_old() {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    for t in 0..3u64 {
        commit(&mut step, &mut out, t, blind(&t.to_be_bytes()), 0);
        sync(&mut step, &mut out, 0);
    }
    assert_eq!(step.watermark(), 6);
    step.step(Input::OldestRead(4), 0, &mut out);
    assert_eq!(step.resolver().history().floor(), 4);
    assert!(out.replies().is_empty() && out.append().is_none());
    commit(
        &mut step,
        &mut out,
        9,
        proposal(2, &Reads::default(), &Writes::default()),
        0,
    );
    sync(&mut step, &mut out, 0);
    assert_eq!(
        out.replies(),
        &[Reply::new(9, Err(CommitError::TooOld(2, 4)))]
    );
    // A floor above the watermark is taken as the watermark: no open transaction reads above it.
    step.step(Input::OldestRead(1_000), 0, &mut out);
    assert_eq!(step.resolver().history().floor(), 6);
}

#[test]
fn test_a_step_opened_after_a_version_follows_it_and_holds_older_reads_too_old() {
    let mut step = Step::new(unbounded(), 100);
    let mut out = Out::new();
    assert_eq!(step.watermark(), 100);
    assert_eq!(step.resolver().history().floor(), 100);
    commit(
        &mut step,
        &mut out,
        1,
        proposal(50, &Reads::default(), &Writes::default()),
        0,
    );
    commit(
        &mut step,
        &mut out,
        2,
        proposal(100, &Reads::default(), &Writes::default()),
        0,
    );
    sync(&mut step, &mut out, 0);
    assert_eq!(
        out.replies(),
        &[Reply::new(1, Err(CommitError::TooOld(50, 100)))]
    );
    sync(&mut step, &mut out, 0);
    assert_eq!(out.replies(), &[Reply::new(2, Ok(102))]);
}

#[test]
fn test_a_clock_stepping_back_between_rounds_does_not_lower_versions() {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    commit(&mut step, &mut out, 1, blind(b"a"), 10_000_000);
    assert_eq!(
        out.append().map(|(r, _)| r.first_version()),
        Some(20_000_000)
    );
    commit(&mut step, &mut out, 2, blind(b"b"), 10_000_000);
    sync(&mut step, &mut out, 9_000_000);
    assert_eq!(
        out.append().map(|(r, _)| r.first_version()),
        Some(20_000_002)
    );
}

#[test]
fn test_two_runs_of_one_round_decide_alike() {
    let run = || {
        let mut step = Step::new(unbounded(), 0);
        let mut out = Out::new();
        let mut replies = Vec::new();
        let mut bodies = Vec::new();
        commit(&mut step, &mut out, 0, blind(b"k"), 0);
        for t in 1..20u64 {
            let key = [b'a' + (t % 5) as u8];
            commit(
                &mut step,
                &mut out,
                t,
                proposal(
                    0,
                    &Reads::list(&key),
                    &Writes::point(&[b'a' + (t % 3) as u8]),
                ),
                0,
            );
        }
        for _ in 0..2 {
            sync(&mut step, &mut out, 0);
            replies.extend(out.replies().iter().cloned());
            bodies.extend(out.append().map(|(_, b)| b.to_vec()));
        }
        (replies, bodies)
    };
    let (first, second) = (run(), run());
    assert_eq!(first, second);
    // And both decide what the round's serial order decides, worked by hand: transaction t reads `a + t % 5` and writes
    // `a + t % 3`, every read at 0, so t aborts on a list conflict once a transaction admitted before it in the round
    // wrote its key. b, c, d and e are free for 1 to 4, which write b, c, a, b; then a, b and c are taken and only the
    // readers of d and e (8, 9, 13, 14, 18, 19) commit. The ten admitted take 4, 6, …, 22 in round order, after the
    // first round's 2.
    let admitted = [1u64, 2, 3, 4, 8, 9, 13, 14, 18, 19];
    let mut expected = vec![Reply::new(0, Ok(2))];
    let mut version = 2;
    for t in 1..20u64 {
        expected.push(if admitted.contains(&t) {
            version += 2;
            Reply::new(t, Ok(version))
        } else {
            Reply::new(t, Err(CommitError::Conflict(ConflictKind::List)))
        });
    }
    assert_eq!(first.0, expected);
}

#[test]
fn test_a_malformed_record_and_an_unsealed_read_set_are_refused() {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    commit(&mut step, &mut out, 0, blind(b"first"), 0);
    let mut bad = blind(b"x");
    bad.record = vec![1, 2, 3];
    commit(&mut step, &mut out, 1, bad, 0);
    let mut unsealed = blind(b"y");
    let mut rs = ReadSet::new();
    rs.reset(0, 0, usize::MAX);
    rs.push_list(b"z").expect("within the limit");
    unsealed.read_set = rs;
    commit(&mut step, &mut out, 2, unsealed, 0);
    sync(&mut step, &mut out, 0);
    sync(&mut step, &mut out, 0);
    let replies = out.replies();
    assert_eq!(replies.len(), 2);
    assert_eq!(replies[0].ticket(), 1);
    assert!(matches!(
        replies[0].outcome(),
        Err(e @ CommitError(ultragraphdb_transactions_commit::CommitErrorEnum::Malformed { .. }))
            if e.code() == ErrorCode::Protocol && e.to_string().starts_with("a write-set record is malformed")
    ));
    assert_eq!(replies[1], Reply::new(2, Err(CommitError::Unsealed())));
}

#[test]
fn test_exhausted_versions_stop_the_step() {
    let mut step = Step::new(unbounded(), u64::MAX - 1);
    let mut out = Out::new();
    commit(&mut step, &mut out, 1, blind(b"a"), 0);
    assert_eq!(
        out.replies(),
        &[Reply::new(1, Err(CommitError::Unavailable()))]
    );
    assert_eq!(out.append(), None);
    assert!(step.is_stopped());
}

/// A blind write of `key` read at u64::MAX − 5, the floor of a step opened there.
fn late(key: &[u8]) -> TestProposal {
    proposal(u64::MAX - 5, &Reads::default(), &Writes::point(key))
}

/// A step after u64::MAX − 5: the first round takes u64::MAX − 3 and the second u64::MAX − 1, the largest even
/// version, so the second round's next transaction finds none.
fn nearly_exhausted() -> (Step, Out) {
    let mut step = Step::new(unbounded(), u64::MAX - 5);
    let mut out = Out::new();
    commit(&mut step, &mut out, 0, late(b"first"), 0);
    assert_eq!(
        out.append().map(|(r, _)| r.first_version()),
        Some(u64::MAX - 3)
    );
    (step, out)
}

#[test]
fn test_versions_exhausted_inside_a_round_refuse_the_rest_and_stop_the_step() {
    let (mut step, mut out) = nearly_exhausted();
    let a = late(b"a");
    let record = a.record.clone();
    commit(&mut step, &mut out, 1, a, 0);
    commit(&mut step, &mut out, 2, late(b"b"), 0);
    sync(&mut step, &mut out, 0);
    assert_eq!(out.replies(), &[Reply::new(0, Ok(u64::MAX - 3))]);
    // Transaction 1 takes the last version; 2 finds none and is refused unavailable in the round's body.
    let (round, body) = out.append().expect("the second round");
    assert_eq!(round, RoundRecord::new(u64::MAX - 1, 2, 1));
    let mut expected = RoundBody::new(0, 0, 0, u64::MAX - 1);
    expected.commit(record);
    expected.abort(ErrorCode::Unavailable, 0);
    assert_eq!(body, expected.body().as_slice());
    assert!(step.is_stopped());
    // Every later commit is refused at once.
    commit(&mut step, &mut out, 3, late(b"c"), 0);
    assert_eq!(
        out.replies(),
        &[Reply::new(3, Err(CommitError::Unavailable()))]
    );
    sync(&mut step, &mut out, 0);
    assert_eq!(
        out.replies(),
        &[
            Reply::new(1, Ok(u64::MAX - 1)),
            Reply::new(2, Err(CommitError::Unavailable()))
        ]
    );
    assert_eq!(applied_versions(&out), vec![u64::MAX - 1]);
    assert_eq!(step.watermark(), u64::MAX - 1);
}

#[test]
fn test_versions_exhausted_at_a_registration_refuse_it_and_stop_the_step() {
    let (mut step, mut out) = nearly_exhausted();
    commit(&mut step, &mut out, 1, late(b"a"), 0);
    step.step(
        Input::Register {
            ticket: 2,
            principal: 0,
        },
        0,
        &mut out,
    );
    sync(&mut step, &mut out, 0);
    assert_eq!(
        out.append().map(|(r, _)| r),
        Some(RoundRecord::new(u64::MAX - 1, 2, 1))
    );
    assert!(step.is_stopped());
    sync(&mut step, &mut out, 0);
    assert_eq!(
        out.replies(),
        &[
            Reply::new(1, Ok(u64::MAX - 1)),
            Reply::new(2, Err(CommitError::Unavailable()))
        ]
    );
    assert!(step.sessions().is_empty(), "no session numbered");
}

#[test]
fn test_the_bounds_follow_the_power_loss_protection_flag() {
    // The node takes the bounds from the log's configuration: 256 transactions a round with protection, 1,024
    // without (WalConfig's table).
    for (protection, held) in [(true, 256u32), (false, 1_024)] {
        let config = WalConfig::new(0, protection);
        let bounds = RoundBounds::new(config.round_bytes(), config.round_transactions());
        let mut step = Step::new(bounds, 0);
        let mut out = Out::new();
        commit(&mut step, &mut out, 0, blind(b"first"), 0);
        for t in 1..=1_100u64 {
            commit(&mut step, &mut out, t, blind(&t.to_be_bytes()), 0);
        }
        sync(&mut step, &mut out, 0);
        assert_eq!(out.append().map(|(r, _)| r.transactions()), Some(held));
    }
}

#[test]
fn test_a_steady_state_reuses_the_output_s_buffers() {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    let mut ticket = 0u64;
    let mut round = |step: &mut Step, out: &mut Out| {
        for _ in 0..16 {
            ticket += 1;
            commit(step, out, ticket, blind(&(ticket % 64).to_be_bytes()), 0);
        }
        sync(step, out, 0);
        out.apply_mut().clear();
        let watermark = step.watermark();
        step.step(Input::OldestRead(watermark), 0, out);
    };
    for _ in 0..100 {
        round(&mut step, &mut out);
    }
    let warm = (out.capacity(), step.resolver().history().capacity());
    for _ in 0..100 {
        round(&mut step, &mut out);
    }
    assert_eq!((out.capacity(), step.resolver().history().capacity()), warm);
}

#[test]
fn test_a_round_closed_by_a_sync_reads_the_clock_then() {
    // The first round at 0 µs takes 2; the next, closed when its sync completes at 10 s, takes 20,000,000.
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    commit(&mut step, &mut out, 1, blind(b"a"), 0);
    commit(&mut step, &mut out, 2, blind(b"b"), 0);
    sync(&mut step, &mut out, 10_000_000);
    assert_eq!(
        out.append().map(|(r, _)| r.first_version()),
        Some(20_000_000)
    );
}

#[test]
fn test_a_malformed_record_is_refused_before_the_read_set_is_checked() {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    let mut p = blind(b"x");
    p.record = vec![1, 2, 3];
    let mut rs = ReadSet::new();
    rs.reset(0, 0, usize::MAX);
    rs.push_list(b"z").expect("within the limit");
    p.read_set = rs;
    commit(&mut step, &mut out, 1, p, 0);
    sync(&mut step, &mut out, 0);
    assert!(matches!(
        out.replies()[0].outcome(),
        Err(CommitError(
            ultragraphdb_transactions_commit::CommitErrorEnum::Malformed { .. }
        ))
    ));
}

/// A transaction that wrote into a range written above its read version, and read nothing there, aborts as a write into
/// a dropped range; one that wrote outside it commits (7.7).
#[test]
fn test_a_blind_write_into_a_range_dropped_after_its_read_aborts() {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    commit(
        &mut step,
        &mut out,
        1,
        proposal(0, &Reads::default(), &Writes::range(b"p", Some(b"q"))),
        0,
    );
    commit(
        &mut step,
        &mut out,
        2,
        proposal(0, &Reads::default(), &Writes::point(b"pa")),
        0,
    );
    commit(
        &mut step,
        &mut out,
        3,
        proposal(0, &Reads::default(), &Writes::statement(b"pb", b"o")),
        0,
    );
    commit(
        &mut step,
        &mut out,
        4,
        proposal(0, &Reads::default(), &Writes::point(b"r")),
        0,
    );
    sync(&mut step, &mut out, 0);
    sync(&mut step, &mut out, 0);
    let outcome = |t: u64| {
        out.replies()
            .iter()
            .find(|r| r.ticket() == t)
            .expect("decided")
            .outcome()
            .clone()
    };
    assert_eq!(outcome(2), Err(CommitError::Conflict(ConflictKind::Drop)));
    assert_eq!(outcome(3), Err(CommitError::Conflict(ConflictKind::Drop)));
    assert_eq!(outcome(4), Ok(4));
}

/// A blind write whose record is exactly `len` bytes, a multiple of 8 as every record is, padded by its commit record.
fn sized(len: usize) -> TestProposal {
    let base = crate::utils::proposal_double::record(0, 0, &Writes::point(b"k"), 0).len();
    let mut p = blind(b"k");
    p.record = crate::utils::proposal_double::record(0, 0, &Writes::point(b"k"), len - base);
    assert_eq!(
        p.record.len(),
        len,
        "a commit record of a multiple of 8 pads byte for byte"
    );
    p
}

#[test]
fn test_on_a_protected_drive_a_record_at_the_largest_write_set_is_taken_and_one_byte_more_is_refused()
 {
    // The power-loss-protected drive's round of 262,144 bytes holds a record of 261,328 alone (RoundBounds' test).
    let config = WalConfig::new(0, true);
    let bounds = RoundBounds::new(config.round_bytes(), config.round_transactions());
    let mut step = Step::new(bounds, 0);
    let mut out = Out::new();
    commit(&mut step, &mut out, 1, sized(261_328), 0);
    assert!(out.replies().is_empty(), "{:?}", out.replies());
    assert_eq!(
        out.append().map(|(r, _)| r),
        Some(RoundRecord::new(2, 1, 1))
    );
    // One byte more, no longer a record: the arrival check reads only the length.
    let mut over = blind(b"k");
    over.record = vec![0; 261_329];
    commit(&mut step, &mut out, 2, over, 0);
    assert_eq!(
        out.replies(),
        &[Reply::new(2, Err(CommitError::TooLarge(261_329, 261_328)))]
    );
    assert_eq!(step.waiting(), 0);
    assert_eq!(CommitError::TooLarge(261_329, 261_328).detail(), 261_328);
}
