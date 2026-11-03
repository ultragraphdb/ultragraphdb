/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{Bind, BindKind, BodyBuilder};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_roles::{golden_role, role_bytes};

/// The transaction identity of the golden role frames: bytes 0xA1 to 0xB0.
pub const TRANSACTION: [u8; 16] = [
    0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xAB, 0xAC, 0xAD, 0xAE, 0xAF, 0xB0,
];

/// The session of the golden role frames: bytes 0x11 to 0x20.
pub const SESSION: [u8; 16] = [
    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F, 0x20,
];

/// The golden Bind reply's watermark, the version (29 << 22) | 500.
pub const WATERMARK: u64 = (29 << 22) | 500;

/// The golden Bind request: partition 3, a transaction of namespace 5, token epoch 30, principal 9.
pub fn golden_bind() -> Bind {
    Bind::new(3, BindKind::Transaction, 5, 30, 9, TRANSACTION)
}

#[test]
fn test_a_bind_builds_the_golden_request_reply_and_drop() -> TestResult {
    let bind = golden_bind();
    assert_eq!(
        (bind.partition(), bind.kind(), bind.namespace()),
        (3, BindKind::Transaction, 5)
    );
    assert_eq!(
        (bind.read_version(), bind.epoch(), bind.principal()),
        (0, 30, 9)
    );
    assert_eq!(bind.transaction(), TRANSACTION);
    let golden = golden_role("bind")?;
    assert_eq!(bind.frame(golden.header())?, role_bytes(&golden)?);
    let reply = bind.with_read_version(WATERMARK);
    assert_eq!(reply.read_version(), WATERMARK);
    let golden_2 = golden_role("bind_reply")?;
    assert_eq!(reply.frame(golden_2.header())?, role_bytes(&golden_2)?);
    let drop = Bind::new(4, BindKind::Drop, 6, 0, 11, SESSION);
    let golden_3 = golden_role("bind_drop")?;
    assert_eq!(drop.frame(golden_3.header())?, role_bytes(&golden_3)?);
    Ok(())
}
