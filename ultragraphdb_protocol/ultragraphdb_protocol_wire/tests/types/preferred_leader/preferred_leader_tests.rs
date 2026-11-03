/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::PreferredLeader;
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_a_leader_encodes_partition_and_replica() -> TestResult {
    let leader = PreferredLeader::new(0x0102_0304, 0x0506_0708);
    assert_eq!(
        (leader.partition(), leader.replica()),
        (0x0102_0304, 0x0506_0708)
    );
    assert_eq!(leader.encode(), [4, 3, 2, 1, 8, 7, 6, 5]);
    assert_eq!(PreferredLeader::SIZE, 8);
    Ok(())
}
