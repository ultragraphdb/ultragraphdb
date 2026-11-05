/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, Held};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_roles::{golden_role, role_bytes};

#[test]
fn test_held_builds_the_golden_frame() -> TestResult {
    let golden = golden_role("held")?;
    assert_eq!(Held.frame(golden.header())?, role_bytes(&golden)?);
    assert_eq!(Held.command_header(), [0; 128]);
    assert!(Held.body().is_empty());
    Ok(())
}
