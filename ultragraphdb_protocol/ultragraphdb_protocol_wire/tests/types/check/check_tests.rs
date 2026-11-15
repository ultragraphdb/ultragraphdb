/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Check's builder (`PROTOCOL.md` §13.24) against the golden `check.frame` and `check_reply.frame` and frames laid
//! out by hand; it refuses what its view refuses.

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Check, CheckKind, CheckOutcome, CheckView, Command, Header,
    MAX_FRAME_SIZE, SessionCheck,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::golden_bodies::{CLUSTER, frame_of};
use crate::utils::golden_roles::{golden_role, role_bytes};
use crate::utils::system_frames::{
    SESSION_A, SESSION_B, SESSION_C, check_frame, check_record, count, field, session_ending,
};

fn header() -> Header {
    Header::new(CLUSTER, Command::Check).with_stream(3)
}

/// The golden request's checks, as `generate_roles.py` lists them in `CHECKS`.
pub fn golden_checks() -> Vec<SessionCheck> {
    vec![
        SessionCheck::new(SESSION_A, 9, CheckKind::Marks),
        SessionCheck::new(SESSION_B, 9, CheckKind::Asks),
        SessionCheck::new(SESSION_C, 11, CheckKind::Marks),
    ]
}

/// The golden reply's outcomes: 1, 12 and 21.
pub const GOLDEN_OUTCOMES: [CheckOutcome; 3] = [
    CheckOutcome::Passed,
    CheckOutcome::NotHeld,
    CheckOutcome::OtherPrincipal,
];

fn refused(checks: Vec<SessionCheck>) -> TestResult<ProtocolErrorEnum> {
    Ok(Check::new(checks)
        .frame(header())
        .err()
        .context("a refusal")?
        .0)
}

/// C1.28: the builder lays out both golden frames.
#[test]
fn test_a_check_builds_the_golden_request_and_reply() -> TestResult {
    let request = Check::new(golden_checks());
    assert_eq!(request.checks(), golden_checks().as_slice());
    let golden = golden_role("check")?;
    assert_eq!(request.frame(golden.header())?, role_bytes(&golden)?);
    let answered: Vec<SessionCheck> = golden_checks()
        .into_iter()
        .zip(GOLDEN_OUTCOMES)
        .map(|(check, outcome)| check.with_outcome(outcome))
        .collect();
    let golden_reply = golden_role("check_reply")?;
    assert_eq!(
        Check::new(answered).frame(golden_reply.header())?,
        role_bytes(&golden_reply)?
    );
    Ok(())
}

/// C1.16 at the builder: each check's 32 bytes, laid out by hand, in the order given.
#[test]
fn test_each_check_is_laid_out_in_32_bytes() -> TestResult {
    let low = session_ending(1);
    let mut high = [0u8; 16];
    high[0] = 1;
    let built = Check::new(vec![
        SessionCheck::new(low, u64::MAX, CheckKind::Asks),
        SessionCheck::new(high, 7, CheckKind::Marks).with_outcome(CheckOutcome::InFlight),
    ])
    .frame(header())?;
    let mut section = check_record(low, u64::MAX, 1, 0);
    section.extend(check_record(high, 7, 2, 14));
    assert_eq!(built, check_frame(section)?);
    Ok(())
}

/// C1.11 at the builder: a Check of no check is refused, and no frame is built.
#[test]
fn test_the_builder_refuses_no_check() -> TestResult {
    assert_eq!(refused(Vec::new())?, count(535, "checks", 0, 1));
    Ok(())
}

/// C1.13 at the builder: a session of all zero is refused naming its check's index.
#[test]
fn test_the_builder_refuses_a_session_of_all_zero() -> TestResult {
    let zero = SessionCheck::new([0; 16], 9, CheckKind::Asks);
    assert_eq!(refused(vec![zero])?, field(535, "check", 0));
    let mut checks = golden_checks();
    checks[2] = zero;
    assert_eq!(refused(checks)?, field(535, "check", 2));
    Ok(())
}

/// C1.14, C1.15 at the builder: a session equal to the one before, or below it, is refused naming the later check.
#[test]
fn test_the_builder_refuses_sessions_that_do_not_ascend() -> TestResult {
    let check = |session| SessionCheck::new(session, 9, CheckKind::Asks);
    assert_eq!(
        refused(vec![check(SESSION_A), check(SESSION_A)])?,
        field(535, "check", 1)
    );
    assert_eq!(
        refused(vec![check(SESSION_B), check(SESSION_A)])?,
        field(535, "check", 1)
    );
    assert_eq!(
        refused(vec![check(SESSION_A), check(SESSION_C), check(SESSION_B)])?,
        field(535, "check", 2)
    );
    Ok(())
}

/// The most checks a frame of 1,048,576 bytes holds, counted from §2.3 and §4.1: 256 bytes of header, 8 of section
/// table, 32 a check, and a chunk table of 8 bytes a chunk once the body passes one chunk of 3,840 bytes.
fn checks_max() -> usize {
    let frame_len = |checks: usize| {
        let data = 8 + 32 * checks;
        let chunks = if data <= 3_840 {
            0
        } else {
            1 + (data - 3_840).div_ceil(4_096)
        };
        256 + data + 8 * chunks
    };
    let mut checks = MAX_FRAME_SIZE / 32;
    while frame_len(checks) > MAX_FRAME_SIZE {
        checks -= 1;
    }
    checks
}

/// Session `index + 1` as 16 big-endian bytes, so sessions ascend with their index.
fn numbered(index: usize) -> [u8; 16] {
    (index as u128 + 1).to_be_bytes()
}

/// C1.23: one check, two, and as many as a frame of 1,048,576 bytes holds are built and read; one more is a frame past
/// 1 MiB.
#[test]
fn test_a_frame_holds_from_one_check_to_as_many_as_fit() -> TestResult {
    let most = checks_max();
    for len in [1, 2, most] {
        let checks: Vec<SessionCheck> = (0..len)
            .map(|index| SessionCheck::new(numbered(index), 9, CheckKind::Marks))
            .collect();
        let bytes = Check::new(checks).frame(header())?;
        assert!(bytes.len() <= MAX_FRAME_SIZE);
        let view = CheckView::from_frame(&frame_of(&bytes)?)?;
        assert_eq!(view.len(), len);
        assert!(!view.is_empty());
        let last = view.get(len - 1).context("the last check")?;
        assert_eq!(last.session(), numbered(len - 1));
        assert_eq!(view.get(len), None);
    }
    let over: Vec<SessionCheck> = (0..=most)
        .map(|index| SessionCheck::new(numbered(index), 9, CheckKind::Marks))
        .collect();
    let refusal = refused(over)?;
    assert!(
        matches!(
            refusal,
            ProtocolErrorEnum::FrameTooLarge {
                limit: MAX_FRAME_SIZE,
                ..
            }
        ),
        "{refusal:?}"
    );
    Ok(())
}
