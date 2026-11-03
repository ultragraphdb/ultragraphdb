/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, Closed, ClosedPartition};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_roles::{golden_role, role_bytes};

#[test]
fn test_the_golden_request_is_built_byte_for_byte() -> TestResult {
    let golden = golden_role("closed")?;
    let request = Closed::request(&[2, 4, 9]);
    assert_eq!(
        request.partitions(),
        &[
            ClosedPartition::new(2, 0),
            ClosedPartition::new(4, 0),
            ClosedPartition::new(9, 0)
        ]
    );
    assert_eq!(request.frame(golden.header())?, role_bytes(&golden)?);
    Ok(())
}

#[test]
fn test_the_golden_reply_is_built_byte_for_byte() -> TestResult {
    let golden = golden_role("closed_reply")?;
    let reply = Closed::new(vec![
        ClosedPartition::new(2, 28),
        ClosedPartition::new(4, 0),
        ClosedPartition::new(9, 31),
    ]);
    assert_eq!(reply.command_header(), [0; 128]);
    assert_eq!(reply.frame(golden.header())?, role_bytes(&golden)?);
    Ok(())
}
