/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, BodyView, Register, RegisterView};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_bodies::{frame_of, golden};

#[test]
fn test_register_builds_and_reads_the_golden_frame() -> TestResult {
    // generate_bodies.py's session: bytes 0x11 to 0x20.
    let session: [u8; 16] = std::array::from_fn(|i| 0x11 + i.to_le_bytes()[0]);
    let register = Register::new(session);
    assert_eq!(register.session(), session);
    let bytes = golden("register")?.bytes()?;
    assert_eq!(register.frame(golden("register")?.header())?, bytes);
    let view = RegisterView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(view.session(), session);
    assert_eq!(view.to_body()?, register);
    assert!(register.body().is_empty());
    Ok(())
}
