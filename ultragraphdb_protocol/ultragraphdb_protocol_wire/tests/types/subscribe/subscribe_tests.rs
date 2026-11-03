/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, Subscribe};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_roles::{golden_role, role_bytes};

/// The golden Subscribe.
pub fn golden_subscribe() -> Subscribe {
    Subscribe::new(41)
}

#[test]
fn test_the_builder_lays_out_the_golden_subscribe() -> TestResult {
    let golden = golden_role("subscribe")?;
    assert_eq!(
        golden_subscribe().frame(golden.header())?,
        role_bytes(&golden)?
    );
    Ok(())
}

#[test]
fn test_the_getter_gives_back_the_position_taken() {
    assert_eq!(golden_subscribe().from(), 41);
    assert_eq!(Subscribe::new(0).from(), 0);
    assert_eq!(Subscribe::new(u64::MAX).from(), u64::MAX);
}
