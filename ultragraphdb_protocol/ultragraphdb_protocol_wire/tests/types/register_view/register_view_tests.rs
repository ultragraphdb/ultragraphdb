/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyBuilder, BodyView, Register, RegisterView};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::golden_bodies::{frame_of, golden, with_command_header};

#[test]
fn test_a_request_carries_session_zero() -> TestResult {
    let bytes = Register::new([0; 16]).frame(golden("register")?.header())?;
    assert_eq!(
        RegisterView::from_frame(&frame_of(&bytes)?)?.session(),
        [0; 16]
    );
    let max = Register::new([0xFF; 16]).frame(golden("register")?.header())?;
    assert_eq!(
        RegisterView::from_frame(&frame_of(&max)?)?.session(),
        [0xFF; 16]
    );
    Ok(())
}

#[test]
fn test_bytes_after_the_session_are_unused() -> TestResult {
    let bytes = golden("register")?.bytes()?;
    for at in [16usize, 127] {
        let frame = with_command_header(&bytes, |h| h[at] = 1);
        assert_eq!(
            RegisterView::from_frame(&frame_of(&frame)?)
                .err()
                .context("a refusal")?
                .0,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    Ok(())
}
