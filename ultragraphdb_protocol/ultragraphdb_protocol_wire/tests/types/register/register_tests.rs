/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, BodyView, Register, RegisterView};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_bodies::{frame_of, golden};

#[test]
fn test_register_builds_and_reads_the_golden_frame() -> TestResult {
    let register = Register::new(0x1122_3344_5566_7788);
    assert_eq!(register.session(), 0x1122_3344_5566_7788);
    let bytes = golden("register")?.bytes()?;
    assert_eq!(register.frame(golden("register")?.header())?, bytes);
    let view = RegisterView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(view.session(), 0x1122_3344_5566_7788);
    assert_eq!(view.to_body()?, register);
    assert!(register.body().is_empty());
    Ok(())
}
