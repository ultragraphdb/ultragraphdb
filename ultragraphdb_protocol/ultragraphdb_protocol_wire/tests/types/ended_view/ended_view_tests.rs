/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Ended's view (`PROTOCOL.md` §13.25), its checks in the Order line's order: command header bytes 0–127 zero;
//! sections; section 0 a whole number of 16 bytes, at least one session; each not all zero and above the one before.

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, Ended, EndedView};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_bodies::{frame_of, view_any, with_command_header};
use crate::utils::system_frames::{
    SESSION_A, SESSION_B, SESSION_C, count, ended_frame, field, reserved, role, session_ending,
};
use crate::utils::term_records::refusal;

fn refused(section: Vec<u8>) -> TestResult<ProtocolErrorEnum> {
    refusal::<EndedView>(&ended_frame(section)?)
}

fn length(len: usize) -> ProtocolErrorEnum {
    ProtocolErrorEnum::SectionLength {
        command: 536,
        section: 0,
        len,
        unit: 16,
    }
}

/// C1.36: the golden frame reads back to its sessions and builds again to the same bytes.
#[test]
fn test_the_golden_ended_reads_and_builds_again() -> TestResult {
    let bytes = role("ended")?;
    let view = EndedView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(view.sessions(), [SESSION_A, SESSION_C]);
    assert_eq!(view.to_body()?, Ended::new(vec![SESSION_A, SESSION_C]));
    assert_eq!(view_any(&bytes)?, Ok(()));
    Ok(())
}

/// C1.29: the command header is all zero.
#[test]
fn test_a_command_header_byte_is_refused() -> TestResult {
    for at in [0usize, 127] {
        let bytes = with_command_header(&role("ended")?, |h| h[at] = 1);
        assert_eq!(refusal::<EndedView>(&bytes)?, reserved(at));
    }
    Ok(())
}

/// C1.30: section 0 is a whole number of 16 bytes.
#[test]
fn test_a_section_of_a_broken_length_is_refused() -> TestResult {
    for len in [15usize, 17, 24] {
        let mut section = [SESSION_A, SESSION_C].concat();
        section.resize(len, 0xEE);
        assert_eq!(refused(section)?, length(len));
    }
    Ok(())
}

/// C1.31: an Ended names at least one session.
#[test]
fn test_no_session_is_refused() -> TestResult {
    assert_eq!(refused(Vec::new())?, count(536, "sessions", 0, 1));
    Ok(())
}

/// C1.32: a session of all zero is refused naming its index.
#[test]
fn test_a_session_of_all_zero_is_refused_with_its_index() -> TestResult {
    assert_eq!(
        refused([[0; 16], SESSION_A].concat())?,
        field(536, "session", 0)
    );
    assert_eq!(
        refused([SESSION_A, [0; 16]].concat())?,
        field(536, "session", 1)
    );
    Ok(())
}

/// C1.33: a session equal to the one before, or below it, is refused naming the later one.
#[test]
fn test_sessions_that_do_not_ascend_are_refused() -> TestResult {
    assert_eq!(
        refused([SESSION_A, SESSION_A].concat())?,
        field(536, "session", 1)
    );
    assert_eq!(
        refused([SESSION_B, SESSION_A].concat())?,
        field(536, "session", 1)
    );
    assert_eq!(
        refused([SESSION_A, SESSION_C, SESSION_B].concat())?,
        field(536, "session", 2)
    );
    Ok(())
}

/// Sessions compare byte by byte from the first: a difference in the last byte alone orders them, and a first byte
/// of 0 lies below a first byte of 1 whatever follows.
#[test]
fn test_sessions_compare_byte_by_byte_from_the_first() -> TestResult {
    let mut low = [0xFFu8; 16];
    low[0] = 0;
    let mut high = [0u8; 16];
    high[0] = 1;
    for (first, second) in [(session_ending(1), session_ending(2)), (low, high)] {
        let bytes = ended_frame([first, second].concat())?;
        assert_eq!(
            EndedView::from_frame(&frame_of(&bytes)?)?.sessions(),
            [first, second]
        );
        assert_eq!(refused([second, first].concat())?, field(536, "session", 1));
    }
    Ok(())
}

/// C1.34: the section's length is checked before any session.
#[test]
fn test_the_section_length_is_checked_before_any_session() -> TestResult {
    assert_eq!(refused(vec![0; 24])?, length(24));
    Ok(())
}

/// C1.35: one session and two read.
#[test]
fn test_one_session_and_two_read() -> TestResult {
    let one = ended_frame(SESSION_B.to_vec())?;
    assert_eq!(
        EndedView::from_frame(&frame_of(&one)?)?.sessions(),
        [SESSION_B]
    );
    let two = ended_frame([SESSION_A, SESSION_B].concat())?;
    assert_eq!(
        EndedView::from_frame(&frame_of(&two)?)?.sessions(),
        [SESSION_A, SESSION_B]
    );
    Ok(())
}

/// A frame of another command is no Ended.
#[test]
fn test_a_frame_of_another_command_is_refused() -> TestResult {
    assert_eq!(
        refusal::<EndedView>(&role("check")?)?,
        ProtocolErrorEnum::WrongCommand {
            expected: 536,
            found: 535
        }
    );
    Ok(())
}
