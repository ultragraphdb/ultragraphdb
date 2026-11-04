/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::runtime::Runtime;
use ultragraphdb_common_role::compute_role;
use ultragraphdb_common_service::{Readiness, RoleIdentity, RoleStart, Stopping};
use ultragraphdb_tool_bdd::{Context, TestResult};

/// A start holding its observability address alone.
fn start() -> TestResult<RoleStart> {
    let text = "[observability]\naddress = \"tcp://127.0.0.1:1\"\n";
    Ok(RoleStart::from_settings(
        text.as_bytes(),
        RoleIdentity::new("ci", "compute", 1),
    )?)
}

/// A compute process names its address.
#[test]
fn test_compute_names_its_address() -> TestResult {
    let refused = Runtime::new()?
        .block_on(compute_role(
            start()?,
            Readiness::default(),
            Stopping::default(),
        ))
        .err()
        .context("a refusal")?;
    assert!(
        refused.to_string().contains("compute.role_address"),
        "{refused}"
    );
    Ok(())
}
