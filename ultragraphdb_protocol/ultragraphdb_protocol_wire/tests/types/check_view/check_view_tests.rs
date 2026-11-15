/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Check's view (`PROTOCOL.md` §13.24), its checks in the Order line's order: command header bytes 0–127 zero;
//! sections; section 0 a whole number of 32-byte checks, at least one; each check in order: bytes 26–31 zero,
//! `session` not all zero and above the one before, `kind`, `outcome`. Every frame is laid out by hand.

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    BodyView, Check, CheckKind, CheckOutcome, CheckView, Command, SessionCheck,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::check::check_tests::{GOLDEN_OUTCOMES, golden_checks};
use crate::utils::golden_bodies::{frame_of, view_any, with_command_header};
use crate::utils::system_frames::{
    SESSION_A, SESSION_B, SESSION_C, check_frame, check_record, count, field, reserved, role,
    session_ending,
};
use crate::utils::term_records::refusal;
use crate::utils::ugs_frames::frame;

/// The body offset of section 0: the one-entry section table before it (§4.1).
const SECTION_0: usize = 8;

/// The three golden checks as their 32-byte records, each changed by `change` given its index.
fn records(change: impl Fn(usize, &mut Vec<u8>)) -> Vec<u8> {
    [(SESSION_A, 9, 2), (SESSION_B, 9, 1), (SESSION_C, 11, 2)]
        .into_iter()
        .enumerate()
        .flat_map(|(index, (session, principal, kind))| {
            let mut record = check_record(session, principal, kind, 0);
            change(index, &mut record);
            record
        })
        .collect()
}

fn refused(section: Vec<u8>) -> TestResult<ProtocolErrorEnum> {
    refusal::<CheckView>(&check_frame(section)?)
}

fn padding(offset: usize) -> ProtocolErrorEnum {
    ProtocolErrorEnum::PaddingNotZero {
        command: 535,
        offset,
    }
}

fn length(len: usize) -> ProtocolErrorEnum {
    ProtocolErrorEnum::SectionLength {
        command: 535,
        section: 0,
        len,
        unit: 32,
    }
}

/// C1.28: the golden frames read back to their checks and build again to the same bytes.
#[test]
fn test_the_golden_checks_read_and_build_again() -> TestResult {
    let request = role("check")?;
    let view = CheckView::from_frame(&frame_of(&request)?)?;
    assert_eq!(view.len(), 3);
    for (index, check) in golden_checks().into_iter().enumerate() {
        assert_eq!(view.get(index), Some(check));
    }
    assert_eq!(view.get(3), None);
    assert_eq!(view.to_body()?, Check::new(golden_checks()));
    let reply = role("check_reply")?;
    let answered = CheckView::from_frame(&frame_of(&reply)?)?;
    for (index, outcome) in GOLDEN_OUTCOMES.into_iter().enumerate() {
        let check = answered.get(index).context("a check")?;
        assert_eq!(check.outcome(), outcome);
        assert_eq!(check, golden_checks()[index].with_outcome(outcome));
    }
    for bytes in [request, reply] {
        assert_eq!(view_any(&bytes)?, Ok(()));
    }
    Ok(())
}

/// C1.8: the command header is all zero.
#[test]
fn test_a_command_header_byte_is_refused() -> TestResult {
    for at in [0usize, 127] {
        let bytes = with_command_header(&role("check")?, |h| h[at] = 1);
        assert_eq!(refusal::<CheckView>(&bytes)?, reserved(at));
    }
    Ok(())
}

/// C1.9: a body of no section is too short for its table, and one of two sections puts section 0 past where one
/// section's table ends (§4.1, steps 1 and 2).
#[test]
fn test_a_body_of_another_number_of_sections_is_refused() -> TestResult {
    let none = frame(Command::Check, &[], &[], false)?;
    assert_eq!(
        refusal::<CheckView>(&none)?,
        ProtocolErrorEnum::BodyTooShort {
            command: 535,
            len: 0,
            need: 8
        }
    );
    let two = frame(
        Command::Check,
        &[],
        &[records(|_, _| ()), Vec::new()],
        false,
    )?;
    assert_eq!(
        refusal::<CheckView>(&two)?,
        ProtocolErrorEnum::SectionOffset {
            command: 535,
            section: 0,
            offset: 16,
            expected: 8
        }
    );
    Ok(())
}

/// C1.10: section 0 is a whole number of 32-byte checks.
#[test]
fn test_a_section_of_a_broken_length_is_refused() -> TestResult {
    for len in [31usize, 33, 48] {
        let mut section = records(|_, _| ());
        section.resize(len, 0);
        assert_eq!(refused(section)?, length(len));
    }
    Ok(())
}

/// C1.11: a Check holds at least one check.
#[test]
fn test_no_check_is_refused() -> TestResult {
    assert_eq!(refused(Vec::new())?, count(535, "checks", 0, 1));
    Ok(())
}

/// C1.12: bytes 26 to 31 of a check are zero, refused naming the byte's body offset.
#[test]
fn test_a_padding_byte_is_refused_at_its_body_offset() -> TestResult {
    for (check, at) in [(0usize, 26usize), (0, 31), (2, 26), (2, 31)] {
        let section = records(|index, record| {
            if index == check {
                record[at] = 1;
            }
        });
        assert_eq!(refused(section)?, padding(SECTION_0 + 32 * check + at));
    }
    Ok(())
}

/// C1.13: a check's session of all zero is refused naming the check's index.
#[test]
fn test_a_session_of_all_zero_is_refused_with_its_index() -> TestResult {
    for check in [0usize, 2] {
        let section = records(|index, record| {
            if index == check {
                record[..16].fill(0);
            }
        });
        assert_eq!(refused(section)?, field(535, "check", check as u64));
    }
    Ok(())
}

/// C1.14, C1.15: a session equal to the one before, or below it, is refused naming the later check.
#[test]
fn test_sessions_that_do_not_ascend_are_refused() -> TestResult {
    let equal = [SESSION_A, SESSION_A];
    let below = [SESSION_B, SESSION_A];
    for sessions in [equal, below] {
        let section: Vec<u8> = sessions
            .into_iter()
            .flat_map(|session| check_record(session, 9, 1, 0))
            .collect();
        assert_eq!(refused(section)?, field(535, "check", 1));
    }
    let later = [SESSION_A, SESSION_C, SESSION_B];
    let section: Vec<u8> = later
        .into_iter()
        .flat_map(|session| check_record(session, 9, 1, 0))
        .collect();
    assert_eq!(refused(section)?, field(535, "check", 2));
    Ok(())
}

/// C1.16: sessions compare byte by byte from the first: a difference in the last byte alone orders them, and a first
/// byte of 0 lies below a first byte of 1 whatever follows.
#[test]
fn test_sessions_compare_byte_by_byte_from_the_first() -> TestResult {
    let mut low = [0xFFu8; 16];
    low[0] = 0;
    low[1] = 1;
    let mut high = [0u8; 16];
    high[0] = 1;
    for (first, second) in [(session_ending(1), session_ending(2)), (low, high)] {
        let mut section = check_record(first, 9, 1, 0);
        section.extend(check_record(second, 9, 1, 0));
        let bytes = check_frame(section)?;
        let view = CheckView::from_frame(&frame_of(&bytes)?)?;
        assert_eq!(view.get(0).map(|check| check.session()), Some(first));
        assert_eq!(view.get(1).map(|check| check.session()), Some(second));
        let mut swapped = check_record(second, 9, 1, 0);
        swapped.extend(check_record(first, 9, 1, 0));
        assert_eq!(refused(swapped)?, field(535, "check", 1));
    }
    Ok(())
}

/// C1.17: a `kind` outside 1 and 2 is refused naming the check's index.
#[test]
fn test_a_kind_outside_its_set_is_refused() -> TestResult {
    for kind in [0u8, 3, 255] {
        for check in [0usize, 1] {
            let section = records(|index, record| {
                if index == check {
                    record[24] = kind;
                }
            });
            assert_eq!(refused(section)?, field(535, "check", check as u64));
        }
    }
    Ok(())
}

/// C1.18: an `outcome` outside 0, 1, 12, 14 and 21 is refused naming the check's index.
#[test]
fn test_an_outcome_outside_its_set_is_refused() -> TestResult {
    for outcome in [2u8, 7, 13, 15, 20, 22, 255] {
        let section = records(|index, record| {
            if index == 1 {
                record[25] = outcome;
            }
        });
        assert_eq!(refused(section)?, field(535, "check", 1), "{outcome}");
    }
    Ok(())
}

/// C1.19: the view reads each outcome of the set, and both kinds.
#[test]
fn test_every_outcome_and_kind_of_the_set_reads() -> TestResult {
    let outcomes = [
        (0u8, CheckOutcome::Unanswered),
        (1, CheckOutcome::Passed),
        (12, CheckOutcome::NotHeld),
        (14, CheckOutcome::InFlight),
        (21, CheckOutcome::OtherPrincipal),
    ];
    for (number, outcome) in outcomes {
        for (kind_number, kind) in [(1u8, CheckKind::Asks), (2, CheckKind::Marks)] {
            let bytes = check_frame(check_record(SESSION_B, 77, kind_number, number))?;
            let view = CheckView::from_frame(&frame_of(&bytes)?)?;
            assert_eq!(view.len(), 1);
            assert_eq!(
                view.get(0),
                Some(SessionCheck::new(SESSION_B, 77, kind).with_outcome(outcome))
            );
        }
    }
    Ok(())
}

/// C1.20: within one check the padding is checked before the session.
#[test]
fn test_the_padding_is_checked_before_the_session() -> TestResult {
    let mut record = check_record([0; 16], 9, 3, 7);
    record[31] = 1;
    assert_eq!(refused(record)?, padding(SECTION_0 + 31));
    Ok(())
}

/// C1.21: the checks are taken in order, each whole before the next.
#[test]
fn test_checks_are_taken_in_order() -> TestResult {
    let section = records(|index, record| match index {
        0 => record[24] = 3,
        1 => record[26] = 1,
        _ => (),
    });
    assert_eq!(refused(section)?, field(535, "check", 0));
    Ok(())
}

/// C1.22: the section's length is checked before any check, and its emptiness before any check too.
#[test]
fn test_the_section_length_is_checked_before_any_check() -> TestResult {
    assert_eq!(refused(vec![0; 33])?, length(33));
    Ok(())
}

/// C1.24: the system group, which receives the request, refuses a check holding an outcome.
#[test]
fn test_a_request_holding_an_outcome_is_refused_by_its_receiver() -> TestResult {
    let request = role("check")?;
    CheckView::from_frame(&frame_of(&request)?)?.check_request()?;
    for outcome in [1u8, 12, 14, 21] {
        for check in [0usize, 2] {
            let section = records(|index, record| {
                if index == check {
                    record[25] = outcome;
                }
            });
            let bytes = check_frame(section)?;
            let refusal = CheckView::from_frame(&frame_of(&bytes)?)?
                .check_request()
                .err()
                .context("a refusal")?;
            assert_eq!(refusal.0, field(535, "check", check as u64));
        }
    }
    Ok(())
}

/// The reply to the golden request whose check `changed` is changed by `change`, as its requester checks it.
fn reply_refusal(
    changed: usize,
    change: impl Fn(&mut Vec<u8>),
) -> TestResult<Option<ProtocolErrorEnum>> {
    let section = records(|index, record| {
        record[25] = 1;
        if index == changed {
            change(record);
        }
    });
    let bytes = check_frame(section)?;
    let view = CheckView::from_frame(&frame_of(&bytes)?)?;
    Ok(view.check_reply(&golden_checks()).err().map(|e| e.0))
}

/// C1.25: the requester refuses a reply holding an outcome of 0, naming the check; a reply of the checks sent, each
/// with an outcome, is taken.
#[test]
fn test_a_reply_without_an_outcome_is_refused_by_the_requester() -> TestResult {
    let reply = role("check_reply")?;
    CheckView::from_frame(&frame_of(&reply)?)?.check_reply(&golden_checks())?;
    for check in [0usize, 1, 2] {
        assert_eq!(
            reply_refusal(check, |record| record[25] = 0)?,
            Some(field(535, "check", check as u64))
        );
    }
    Ok(())
}

/// C1.26: the requester refuses a reply whose check is not the one it sent at that index.
#[test]
fn test_a_reply_of_another_check_is_refused_by_the_requester() -> TestResult {
    let other_session: fn(&mut Vec<u8>) = |record| record[15] ^= 0x01;
    let other_principal: fn(&mut Vec<u8>) = |record| record[16] ^= 0x01;
    let other_kind: fn(&mut Vec<u8>) = |record| record[24] ^= 0x03;
    for change in [other_session, other_principal, other_kind] {
        for check in [0usize, 2] {
            assert_eq!(
                reply_refusal(check, change)?,
                Some(field(535, "check", check as u64))
            );
        }
    }
    assert_eq!(reply_refusal(1, |_| ())?, None);
    Ok(())
}

/// C1.27: a reply holding another number of checks than its request is refused before any check is compared.
#[test]
fn test_a_reply_of_another_count_is_refused_before_any_check() -> TestResult {
    let sent = golden_checks();
    // Two checks, the first with no outcome: the count is refused, not the check.
    let mut fewer = check_record(SESSION_A, 9, 2, 0);
    fewer.extend(check_record(SESSION_B, 9, 1, 1));
    let bytes = check_frame(fewer)?;
    let refusal = CheckView::from_frame(&frame_of(&bytes)?)?
        .check_reply(&sent)
        .err()
        .context("a refusal")?;
    assert_eq!(refusal.0, count(535, "checks", 2, 3));
    let reply = role("check_reply")?;
    let more = CheckView::from_frame(&frame_of(&reply)?)?
        .check_reply(&sent[..2])
        .err()
        .context("a refusal")?;
    assert_eq!(more.0, count(535, "checks", 3, 2));
    Ok(())
}

/// A frame of another command is no Check.
#[test]
fn test_a_frame_of_another_command_is_refused() -> TestResult {
    assert_eq!(
        refusal::<CheckView>(&role("ended")?)?,
        ProtocolErrorEnum::WrongCommand {
            expected: 535,
            found: 536
        }
    );
    Ok(())
}
