/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, Cancel};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_bodies::golden;

#[test]
fn test_cancel_builds_the_golden_frame() -> TestResult {
    assert_eq!(
        Cancel.frame(golden("cancel")?.header())?,
        golden("cancel")?.bytes()?
    );
    Ok(())
}
