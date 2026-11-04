/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_storage::partition_address;
use ultragraphdb_error_role::RoleError;
use ultragraphdb_tool_bdd::{Context, TestResult};

/// Over TCP each partition takes the next port; over a Unix socket a path of its own.
#[test]
fn test_each_partition_has_an_address_of_its_own() -> TestResult {
    assert_eq!(
        partition_address("tcp://127.0.0.1:7000", 3, 3)?,
        "tcp://127.0.0.1:7000"
    );
    assert_eq!(
        partition_address("tcp://127.0.0.1:7000", 3, 5)?,
        "tcp://127.0.0.1:7002"
    );
    assert_eq!(
        partition_address("unix:///run/ultragraph/storage.sock", 1, 2)?,
        "unix:///run/ultragraph/storage-p2.sock"
    );
    assert_eq!(
        partition_address("unix:///run/s", 1, 1)?,
        "unix:///run/s-p1"
    );
    Ok(())
}

/// A port past the last is the configuration's fault, as is an address that does not parse.
#[test]
fn test_an_address_past_the_ports_is_refused() -> TestResult {
    for (base, partition) in [("tcp://127.0.0.1:65535", 2), ("http://nowhere", 1)] {
        let refused = partition_address(base, 1, partition)
            .err()
            .context("a refusal")?;
        assert_eq!(refused.exit_code(), RoleError::EX_CONFIG, "{base}");
    }
    Ok(())
}
