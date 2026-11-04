/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_service_config::partition_address;
use ultragraphdb_tool_bdd::{Context, TestResult};

/// Over TCP each partition takes the next port; over a Unix socket a path of its own.
#[test]
fn test_each_partition_has_an_address_of_its_own() -> TestResult {
    assert_eq!(
        partition_address("tcp://127.0.0.1:7000", 3, 3),
        Ok("tcp://127.0.0.1:7000".to_string())
    );
    assert_eq!(
        partition_address("tcp://127.0.0.1:7000", 3, 5),
        Ok("tcp://127.0.0.1:7002".to_string())
    );
    assert_eq!(
        partition_address("unix:///run/ultragraph/storage.sock", 1, 2),
        Ok("unix:///run/ultragraph/storage-p2.sock".to_string())
    );
    assert_eq!(
        partition_address("unix:///run/s", 1, 1),
        Ok("unix:///run/s-p1".to_string())
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
        assert!(refused.contains(base), "{refused}");
    }
    Ok(())
}
