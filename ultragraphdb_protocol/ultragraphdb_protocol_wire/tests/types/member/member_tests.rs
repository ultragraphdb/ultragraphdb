/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{Member, RoleKind};
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_a_member_encodes_its_fields() -> TestResult {
    let storage = Member::new(RoleKind::Storage, 0x0102_0304, 0x0A0B_0C0D);
    assert_eq!(
        (storage.kind(), storage.partition(), storage.address()),
        (RoleKind::Storage, 0x0102_0304, 0x0A0B_0C0D)
    );
    assert_eq!(
        storage.encode(),
        [3, 0, 0, 0, 4, 3, 2, 1, 0x0D, 0x0C, 0x0B, 0x0A]
    );
    assert_eq!(
        Member::new(RoleKind::Compute, 0, 7).encode(),
        [2, 0, 0, 0, 0, 0, 0, 0, 7, 0, 0, 0]
    );
    assert_eq!(Member::SIZE, 12);
    Ok(())
}
