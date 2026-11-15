/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Session's view (`PROTOCOL.md` §13.23), its checks in the Order line's order: unused bytes 24–127 zero; `session`;
//! sections.

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, Session, SessionView};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_bodies::{frame_of, put, reframe, view_any, with_command_header};
use crate::utils::system_frames::{
    SESSION_A, field, reserved, role, session_ending, session_frame,
};
use crate::utils::term_records::refusal;

/// C1.7: the golden frame reads back to its fields and builds again to the same bytes; a reply is the same bytes.
#[test]
fn test_the_golden_session_reads_and_builds_again() -> TestResult {
    let bytes = role("session")?;
    let view = SessionView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!((view.session(), view.principal()), (SESSION_A, 9));
    assert_eq!(view.to_body()?, Session::new(SESSION_A, 9));
    assert_eq!(view_any(&bytes)?, Ok(()));
    Ok(())
}

/// C1.1: an unused byte, the first and the last, is refused naming its frame offset.
#[test]
fn test_unused_bytes_are_refused() -> TestResult {
    for at in [24usize, 127] {
        let bytes = with_command_header(&role("session")?, |h| h[at] = 1);
        assert_eq!(refusal::<SessionView>(&bytes)?, reserved(at));
    }
    Ok(())
}

/// C1.2: a session of all zero names no session.
#[test]
fn test_a_session_of_all_zero_is_refused() -> TestResult {
    let bytes = session_frame([0; 16], 9)?;
    assert_eq!(refusal::<SessionView>(&bytes)?, field(534, "session", 0));
    Ok(())
}

/// C1.3: the unused bytes are checked before the session.
#[test]
fn test_an_unused_byte_is_refused_before_the_session() -> TestResult {
    let bytes = with_command_header(&session_frame([0; 16], 9)?, |h| h[24] = 1);
    assert_eq!(refusal::<SessionView>(&bytes)?, reserved(24));
    Ok(())
}

/// C1.4: the body holds no section, so any byte of body trails the empty section table (§4.1, step 3); the session is
/// checked before the sections.
#[test]
fn test_a_body_holding_a_section_is_refused_after_the_session() -> TestResult {
    let bytes = reframe(&role("session")?, &[0; 8]);
    assert_eq!(
        refusal::<SessionView>(&bytes)?,
        ProtocolErrorEnum::TrailingBytes {
            command: 534,
            end: 0,
            len: 8
        }
    );
    let zero = reframe(&session_frame([0; 16], 9)?, &[0; 8]);
    assert_eq!(refusal::<SessionView>(&zero)?, field(534, "session", 0));
    Ok(())
}

/// C1.5: a session is refused only when every byte is zero.
#[test]
fn test_a_session_of_one_nonzero_byte_reads() -> TestResult {
    for session in [session_ending(1), [0xFF; 16]] {
        let bytes = session_frame(session, 9)?;
        assert_eq!(
            SessionView::from_frame(&frame_of(&bytes)?)?.session(),
            session
        );
    }
    Ok(())
}

/// C1.6: the body states no rule for `principal`.
#[test]
fn test_any_principal_reads() -> TestResult {
    for principal in [0, u64::MAX] {
        let bytes = with_command_header(&role("session")?, |h| put(h, 16, principal, 8));
        assert_eq!(
            SessionView::from_frame(&frame_of(&bytes)?)?.principal(),
            principal
        );
    }
    Ok(())
}

/// A frame of another command is no Session.
#[test]
fn test_a_frame_of_another_command_is_refused() -> TestResult {
    assert_eq!(
        refusal::<SessionView>(&role("held")?)?,
        ProtocolErrorEnum::WrongCommand {
            expected: 534,
            found: 531
        }
    );
    Ok(())
}
