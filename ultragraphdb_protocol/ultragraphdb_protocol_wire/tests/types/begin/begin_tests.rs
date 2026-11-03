/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Epoch, SessionToken};
use ultragraphdb_protocol_wire::{Begin, BodyBuilder, ReadLevel};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_bodies::golden;

#[test]
fn test_begin_builds_the_golden_frame() -> TestResult {
    // PROTOCOL.md section 6.2: read-only at level 2, a 50 ms deadline, and partition 2's token at position 41 in
    // epoch 30.
    let begin = Begin::new(3, 0, true)
        .with_level(ReadLevel::MonotonicFast)
        .with_deadline_ms(50)
        .with_token(SessionToken::new(2, 41, Epoch::new(30)?));
    assert_eq!(
        (begin.namespace(), begin.read_version(), begin.read_only()),
        (3, 0, true)
    );
    assert_eq!(
        (begin.level(), begin.deadline_ms()),
        (ReadLevel::MonotonicFast, 50)
    );
    assert_eq!(
        begin.frame(golden("begin")?.header())?,
        golden("begin")?.bytes()?
    );
    // Read-write is mode 0.
    assert_eq!(Begin::new(3, 0, false).command_header()[16], 0);
    assert_eq!(begin.command_header()[16], 1);
    Ok(())
}

#[test]
fn test_a_read_write_begin_reads_back_as_not_read_only() {
    // Constructed read-write, the getter gives back what was given.
    assert!(!Begin::new(3, 0, false).read_only());
}
