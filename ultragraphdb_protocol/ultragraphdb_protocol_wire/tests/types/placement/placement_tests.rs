/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, BucketRange, Placement, PreferredLeader, TenantRun};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_roles::{golden_role, role_bytes};

/// The golden Placement request.
pub fn golden_placement() -> Placement {
    Placement::request(42)
}

#[test]
fn test_the_builder_lays_out_the_golden_placement() -> TestResult {
    let golden = golden_role("placement")?;
    assert_eq!(
        golden_placement().frame(golden.header())?,
        role_bytes(&golden)?
    );
    Ok(())
}

/// The golden Placement reply.
pub fn golden_placement_reply() -> Placement {
    let mut map = Placement::reply(1_234, 42, 40, 45);
    map.push_range(BucketRange::new(0, 0x40_0000, 1));
    map.push_range(BucketRange::new(0x40_0000, 0x80_0000, 2));
    map.push_range(BucketRange::new(0x80_0000, 1 << 24, 1));
    map.push_run(TenantRun::new(3, 0x40));
    map.push_run(TenantRun::new(7, 0x81));
    map.push_leader(PreferredLeader::new(1, 3));
    map.push_leader(PreferredLeader::new(2, 4));
    map
}

#[test]
fn test_the_builder_lays_out_the_golden_placement_reply() -> TestResult {
    let golden = golden_role("placement_reply")?;
    assert_eq!(
        golden_placement_reply().frame(golden.header())?,
        role_bytes(&golden)?
    );
    Ok(())
}

#[test]
fn test_the_getters_give_back_what_the_constructors_and_pushes_took() {
    let request = golden_placement();
    assert_eq!(
        (
            request.position(),
            request.epoch(),
            request.from(),
            request.until()
        ),
        (0, 42, 0, 0)
    );
    assert!(
        request.ranges().is_empty() && request.runs().is_empty() && request.leaders().is_empty()
    );
    let reply = golden_placement_reply();
    assert_eq!(
        (reply.position(), reply.epoch(), reply.from(), reply.until()),
        (1_234, 42, 40, 45)
    );
    assert_eq!(
        reply.ranges(),
        [
            BucketRange::new(0, 0x40_0000, 1),
            BucketRange::new(0x40_0000, 0x80_0000, 2),
            BucketRange::new(0x80_0000, 1 << 24, 1)
        ]
        .as_slice()
    );
    assert_eq!(
        reply.runs(),
        [TenantRun::new(3, 0x40), TenantRun::new(7, 0x81)].as_slice()
    );
    assert_eq!(
        reply.leaders(),
        [PreferredLeader::new(1, 3), PreferredLeader::new(2, 4)].as_slice()
    );
}
