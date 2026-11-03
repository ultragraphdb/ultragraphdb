/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, Lease};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_roles::{golden_role, role_bytes};

/// The golden Lease request.
pub fn golden_lease() -> Lease {
    Lease::request(2, 5, 4096)
}

#[test]
fn test_the_builder_lays_out_the_golden_lease() -> TestResult {
    let golden = golden_role("lease")?;
    assert_eq!(golden_lease().frame(golden.header())?, role_bytes(&golden)?);
    Ok(())
}

/// The golden Lease reply.
pub fn golden_lease_reply() -> Lease {
    Lease::request(2, 5, 4096).with_block(81_921)
}

#[test]
fn test_the_builder_lays_out_the_golden_lease_reply() -> TestResult {
    let golden = golden_role("lease_reply")?;
    assert_eq!(
        golden_lease_reply().frame(golden.header())?,
        role_bytes(&golden)?
    );
    Ok(())
}
