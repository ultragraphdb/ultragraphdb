/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, Command, Header, Subscribe};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_bodies::CLUSTER;
use crate::utils::golden_roles::{golden_role, role_bytes};
use crate::utils::ugs_frames::frame;

/// The golden Subscribe.
pub fn golden_subscribe() -> Subscribe {
    Subscribe::new(41)
}

#[test]
fn test_the_builder_lays_out_the_golden_subscribe() -> TestResult {
    let golden = golden_role("subscribe")?;
    assert_eq!(
        golden_subscribe().frame(golden.header())?,
        role_bytes(&golden)?
    );
    Ok(())
}

#[test]
fn test_the_getter_gives_back_the_position_taken() {
    assert_eq!(golden_subscribe().from(), 41);
    assert_eq!(Subscribe::new(0).from(), 0);
    assert_eq!(Subscribe::new(u64::MAX).from(), u64::MAX);
}

/// C1.98 at the builder: a subscription from the latest close is `latest` 1 at offset 8 with `from` 0, the frame laid
/// out by hand; the constructor in use builds `latest` 0.
#[test]
fn test_a_subscribe_from_the_latest_close_is_laid_out() -> TestResult {
    let latest = Subscribe::at_latest();
    assert_eq!((latest.from(), latest.latest()), (0, true));
    let built = latest.frame(Header::new(CLUSTER, Command::Subscribe).with_stream(3))?;
    assert_eq!(built, frame(Command::Subscribe, &[(8, &[1])], &[], false)?);
    assert!(!golden_subscribe().latest());
    assert!(!Subscribe::new(0).latest());
    Ok(())
}
