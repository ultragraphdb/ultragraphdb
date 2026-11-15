/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Session's builder (`PROTOCOL.md` §13.23) against the golden `session.frame` and frames laid out by hand.

use ultragraphdb_protocol_wire::{BodyBuilder, Command, Header, Session};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::golden_bodies::CLUSTER;
use crate::utils::golden_roles::{golden_role, role_bytes};
use crate::utils::system_frames::{SESSION_A, field, session_ending, session_frame};

/// The header `utils::ugs_frames::frame` lays its frames out under.
fn header() -> Header {
    Header::new(CLUSTER, Command::Session).with_stream(3)
}

/// C1.7: the builder lays out the golden frame, `generate_roles.py`'s session `0x11..0x20` for principal 9.
#[test]
fn test_a_session_builds_the_golden_frame() -> TestResult {
    let session = Session::new(SESSION_A, 9);
    assert_eq!((session.session(), session.principal()), (SESSION_A, 9));
    let golden = golden_role("session")?;
    assert_eq!(session.frame(golden.header())?, role_bytes(&golden)?);
    Ok(())
}

/// C1.5, C1.6: the fields sit at offsets 0 and 16, whatever the principal.
#[test]
fn test_the_fields_sit_at_their_offsets() -> TestResult {
    for (session, principal) in [
        (session_ending(1), 0),
        ([0xFF; 16], u64::MAX),
        (SESSION_A, 0x0102_0304_0506_0708),
    ] {
        assert_eq!(
            Session::new(session, principal).frame(header())?,
            session_frame(session, principal)?
        );
    }
    Ok(())
}

/// C1.2 at the builder: a session of all zero is refused as its view refuses it, and no frame is built.
#[test]
fn test_the_builder_refuses_a_session_of_all_zero() -> TestResult {
    let refusal = Session::new([0; 16], 9)
        .frame(header())
        .err()
        .context("a refusal")?;
    assert_eq!(refusal.0, field(534, "session", 0));
    Ok(())
}
