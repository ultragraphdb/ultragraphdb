/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, Held, HeldView};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_bodies::{frame_of, reframe, view_any, with_command_header};
use crate::utils::golden_roles::{golden_role, role_bytes};
use crate::utils::term_records::refusal;

fn golden() -> TestResult<Vec<u8>> {
    role_bytes(&golden_role("held")?)
}

#[test]
fn test_the_golden_held_reads_back_to_its_body() -> TestResult {
    let bytes = golden()?;
    assert_eq!(HeldView::from_frame(&frame_of(&bytes)?)?.to_body()?, Held);
    assert_eq!(view_any(&bytes)?, Ok(()));
    Ok(())
}

#[test]
fn test_every_command_header_byte_is_zero() -> TestResult {
    for at in [0usize, 127] {
        let bytes = with_command_header(&golden()?, |h| h[at] = 1);
        assert_eq!(
            refusal::<HeldView>(&bytes)?,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    Ok(())
}

#[test]
fn test_a_held_frame_holds_no_section() -> TestResult {
    let bytes = reframe(&golden()?, &[0]);
    assert!(matches!(
        refusal::<HeldView>(&bytes)?,
        ProtocolErrorEnum::TrailingBytes { command: 531, .. }
    ));
    Ok(())
}
