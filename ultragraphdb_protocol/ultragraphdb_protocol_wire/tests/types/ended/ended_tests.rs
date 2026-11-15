/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Ended's builder (`PROTOCOL.md` §13.25) against the golden `ended.frame` and frames laid out by hand; it refuses
//! what its view refuses.

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, Ended, EndedView, FLAG_LAST, Header, MAX_FRAME_SIZE,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::golden_bodies::{CLUSTER, frame_of};
use crate::utils::golden_roles::{golden_role, role_bytes};
use crate::utils::system_frames::{
    SESSION_A, SESSION_B, SESSION_C, count, ended_frame, field, session_ending,
};

fn header() -> Header {
    Header::new(CLUSTER, Command::Ended)
        .with_stream(3)
        .with_flags(FLAG_LAST)
}

fn refused(sessions: Vec<[u8; 16]>) -> TestResult<ProtocolErrorEnum> {
    Ok(Ended::new(sessions)
        .frame(header())
        .err()
        .context("a refusal")?
        .0)
}

/// C1.36: the builder lays out the golden frame, `generate_roles.py`'s sessions `0x11..0x20` and `0xA1..0xB0`.
#[test]
fn test_an_ended_builds_the_golden_frame() -> TestResult {
    let ended = Ended::new(vec![SESSION_A, SESSION_C]);
    assert_eq!(ended.sessions(), [SESSION_A, SESSION_C]);
    let golden = golden_role("ended")?;
    assert_eq!(ended.frame(golden.header())?, role_bytes(&golden)?);
    Ok(())
}

/// The sessions sit back to back in section 0, in the order given.
#[test]
fn test_the_sessions_sit_back_to_back() -> TestResult {
    let sessions = vec![session_ending(1), session_ending(2), [0xFF; 16]];
    let built = Ended::new(sessions.clone()).frame(header())?;
    assert_eq!(built, ended_frame(sessions.concat())?);
    Ok(())
}

/// C1.31 at the builder: an Ended of no session is refused, and no frame is built.
#[test]
fn test_the_builder_refuses_no_session() -> TestResult {
    assert_eq!(refused(Vec::new())?, count(536, "sessions", 0, 1));
    Ok(())
}

/// C1.32, C1.33 at the builder: a session of all zero, equal to the one before or below it is refused naming its
/// index.
#[test]
fn test_the_builder_refuses_what_the_view_refuses() -> TestResult {
    assert_eq!(refused(vec![[0; 16]])?, field(536, "session", 0));
    assert_eq!(refused(vec![SESSION_A, [0; 16]])?, field(536, "session", 1));
    assert_eq!(
        refused(vec![SESSION_A, SESSION_A])?,
        field(536, "session", 1)
    );
    assert_eq!(
        refused(vec![SESSION_A, SESSION_C, SESSION_B])?,
        field(536, "session", 2)
    );
    Ok(())
}

/// The most sessions a frame of 1,048,576 bytes holds, counted from §2.3 and §4.1: 256 bytes of header, 8 of section
/// table, 16 a session, and a chunk table of 8 bytes a chunk once the body passes one chunk of 3,840 bytes.
fn sessions_max() -> usize {
    let frame_len = |sessions: usize| {
        let data = 8 + 16 * sessions;
        let chunks = if data <= 3_840 {
            0
        } else {
            1 + (data - 3_840).div_ceil(4_096)
        };
        256 + data + 8 * chunks
    };
    let mut sessions = MAX_FRAME_SIZE / 16;
    while frame_len(sessions) > MAX_FRAME_SIZE {
        sessions -= 1;
    }
    sessions
}

/// Session `index + 1` as 16 big-endian bytes, so sessions ascend with their index.
fn numbered(index: usize) -> [u8; 16] {
    (index as u128 + 1).to_be_bytes()
}

/// C1.35: one session, two, and as many as a frame of 1,048,576 bytes holds are built and read; one more is a frame
/// past 1 MiB.
#[test]
fn test_a_frame_holds_from_one_session_to_as_many_as_fit() -> TestResult {
    let most = sessions_max();
    for len in [1, 2, most] {
        let sessions: Vec<[u8; 16]> = (0..len).map(numbered).collect();
        let bytes = Ended::new(sessions.clone()).frame(header())?;
        assert!(bytes.len() <= MAX_FRAME_SIZE);
        let view = EndedView::from_frame(&frame_of(&bytes)?)?;
        assert_eq!(view.sessions(), sessions.as_slice());
    }
    let refusal = refused((0..=most).map(numbered).collect())?;
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
