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

#[test]
fn test_the_getters_give_back_what_the_constructors_took() {
    let request = golden_lease();
    assert_eq!(
        (
            request.type_prefix(),
            request.partition(),
            request.size(),
            request.start(),
            request.end()
        ),
        (2, 5, 4096, 0, 0)
    );
    let reply = golden_lease_reply();
    assert_eq!(
        (
            reply.type_prefix(),
            reply.partition(),
            reply.size(),
            reply.start(),
            reply.end()
        ),
        (2, 5, 4096, 81_921, 81_921 + 4096)
    );
}

#[test]
fn test_a_block_ending_past_u64_saturates() {
    let at_max = Lease::request(1, 1, 10).with_block(u64::MAX - 10);
    assert_eq!((at_max.start(), at_max.end()), (u64::MAX - 10, u64::MAX));
    let past = Lease::request(1, 1, 10).with_block(u64::MAX - 9);
    assert_eq!((past.start(), past.end()), (u64::MAX - 9, u64::MAX));
}
