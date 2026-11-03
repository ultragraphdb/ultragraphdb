/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, Members};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_roles::{golden_role, role_bytes};

/// The golden Members request.
pub fn golden_members() -> Members {
    Members::new()
}

#[test]
fn test_the_builder_lays_out_the_golden_members() -> TestResult {
    let golden = golden_role("members")?;
    assert_eq!(
        golden_members().frame(golden.header())?,
        role_bytes(&golden)?
    );
    Ok(())
}

/// The golden Members reply.
pub fn golden_members_reply() -> Members {
    let mut members = Members::new();
    members.push_compute("tcp://compute-a:7400");
    members.push_compute("tcp://compute-b:7400");
    members.push_storage(2, "tcp://storage-1:7300");
    members.push_storage(5, "tcp://storage-1:7300");
    members.push_storage(9, "tcp://storage-2:7300");
    members
}

#[test]
fn test_the_builder_lays_out_the_golden_members_reply() -> TestResult {
    let golden = golden_role("members_reply")?;
    assert_eq!(
        golden_members_reply().frame(golden.header())?,
        role_bytes(&golden)?
    );
    Ok(())
}
