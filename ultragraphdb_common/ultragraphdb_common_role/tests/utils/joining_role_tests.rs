/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::runtime::Runtime;
use ultragraphdb_common_role::{compute_role, storage_role};
use ultragraphdb_common_service::{Readiness, RoleStart};
use ultragraphdb_error_role::RoleError;
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

/// A storage process names its address and at least one partition.
#[test]
fn test_storage_names_its_address_and_partitions() -> TestResult {
    let runtime = Runtime::new()?;
    for (settings, named) in [
        (vec![], "storage.role_address"),
        (
            vec![("ULTRAGRAPHDB_STORAGE_ROLE_ADDRESS", "tcp://127.0.0.1:0")],
            "storage.partition_count",
        ),
        (
            vec![
                ("ULTRAGRAPHDB_STORAGE_ROLE_ADDRESS", "tcp://127.0.0.1:0"),
                ("ULTRAGRAPHDB_STORAGE_PARTITION_COUNT", "0"),
            ],
            "storage.partition_count",
        ),
    ] {
        let refused = runtime
            .block_on(storage_role(start(&settings)?, Readiness::default()))
            .err()
            .context("a refusal")?;
        assert_eq!(refused.exit_code(), RoleError::EX_CONFIG);
        assert!(refused.to_string().contains(named), "{refused}");
    }
    Ok(())
}

/// A compute process names its address.
#[test]
fn test_compute_names_its_address() -> TestResult {
    let refused = Runtime::new()?
        .block_on(compute_role(start(&[])?, Readiness::default()))
        .err()
        .context("a refusal")?;
    assert!(
        refused.to_string().contains("compute.role_address"),
        "{refused}"
    );
    Ok(())
}
