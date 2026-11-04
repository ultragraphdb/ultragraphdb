/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::runtime::Runtime;
use ultragraphdb_common_role::compute_role;
use ultragraphdb_common_service::{Readiness, RoleStart, Stopping};
use ultragraphdb_tool_bdd::{Context, TestResult};

fn start(settings: &[(&str, &str)]) -> TestResult<RoleStart> {
    let mut variables = vec![(
        "ULTRAGRAPHDB_OBSERVABILITY_ADDRESS".to_string(),
        "tcp://127.0.0.1:1".to_string(),
    )];
    variables.extend(
        settings
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string())),
    );
    Ok(RoleStart::read(&[], variables)?)
}

/// A compute process names its address.
#[test]
fn test_compute_names_its_address() -> TestResult {
    let refused = Runtime::new()?
        .block_on(compute_role(
            start(&[])?,
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
