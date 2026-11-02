/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{Abort, BodyBuilder};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_bodies::golden;

#[test]
fn test_abort_builds_the_golden_frame() -> TestResult {
    assert_eq!(
        Abort.frame(golden("abort")?.header())?,
        golden("abort")?.bytes()?
    );
    assert_eq!(Abort.command_header(), [0; 128]);
    assert!(Abort.body().is_empty());
    Ok(())
}
