/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, Recall};
use ultragraphdb_tool_bdd::TestResult;

use crate::types::bind::bind_tests::SESSION;
use crate::utils::golden_roles::{golden_role, role_bytes};

#[test]
fn test_a_recall_builds_the_golden_frame() -> TestResult {
    let recall = Recall::new(3, SESSION, 8);
    assert_eq!(
        (recall.partition(), recall.session(), recall.request()),
        (3, SESSION, 8)
    );
    let golden = golden_role("recall")?;
    assert_eq!(recall.frame(golden.header())?, role_bytes(&golden)?);
    Ok(())
}
