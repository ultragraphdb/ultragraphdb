/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, CatalogAnnouncement, EpochClose};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_roles::{golden_role, role_bytes};

/// The golden EpochClose.
pub fn golden_epoch_close() -> EpochClose {
    let mut close = EpochClose::new(42, 1_798_761_600_123_456).with_more();
    for (namespace, version) in [(3, 11), (3, 12), (7, 4)] {
        close.push_announcement(CatalogAnnouncement::new(namespace, version));
    }
    close.push_eviction(std::array::from_fn(|i| 0x11 + i as u8));
    close.push_eviction(std::array::from_fn(|i| 0x31 + i as u8));
    close
}

#[test]
fn test_the_builder_lays_out_the_golden_epoch_close() -> TestResult {
    let golden = golden_role("epoch_close")?;
    assert_eq!(
        golden_epoch_close().frame(golden.header())?,
        role_bytes(&golden)?
    );
    Ok(())
}
