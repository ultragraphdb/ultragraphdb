/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, Scan, ScanAt, ScanRange};
use ultragraphdb_tool_bdd::TestResult;

use crate::types::bind::bind_tests::{TRANSACTION, WATERMARK};
use crate::utils::golden_roles::{golden_role, role_bytes};

/// The golden Scan: at 1, the version bound of epoch 30, once epoch 30 closed, principal 9, no transaction, 1,000
/// entries per range; partition 3 from 0x10 `abc` to 0x10 `abd`, partition 4 from 0x20 `k` to its keyspace's end.
pub fn golden_scan() -> Scan {
    let mut scan = Scan::new(
        ScanAt::ReadVersion,
        (30 << 22) | 0x3F_FFFF,
        30,
        9,
        [0; 16],
        1_000,
    );
    scan.push_range(ScanRange::new(3, b"\x10abc", b"\x10abd"));
    scan.push_range(ScanRange::new(4, b"\x20k", b""));
    scan
}

/// The golden read-write transaction's Scan: the one key 0x30 `key` of partition 3 at the Bind's watermark.
pub fn golden_scan_transaction() -> Scan {
    let mut scan = Scan::new(ScanAt::ReadVersion, WATERMARK, 0, 9, TRANSACTION, 0);
    scan.push_range(ScanRange::new(3, b"\x30key", b"\x30key\x00"));
    scan
}

#[test]
fn test_a_scan_builds_the_golden_frames() -> TestResult {
    let scan = golden_scan();
    assert_eq!(
        (scan.at(), scan.read_version(), scan.epoch()),
        (ScanAt::ReadVersion, (30 << 22) | 0x3F_FFFF, 30)
    );
    assert_eq!(
        (scan.principal(), scan.transaction(), scan.limit()),
        (9, [0; 16], 1_000)
    );
    let ranges: Vec<ScanRange<'_>> = scan.ranges().collect();
    assert_eq!(
        ranges,
        vec![
            ScanRange::new(3, b"\x10abc", b"\x10abd"),
            ScanRange::new(4, b"\x20k", b"")
        ]
    );
    let golden = golden_role("scan")?;
    assert_eq!(scan.frame(golden.header())?, role_bytes(&golden)?);
    let golden_2 = golden_role("scan_transaction")?;
    assert_eq!(
        golden_scan_transaction().frame(golden_2.header())?,
        role_bytes(&golden_2)?
    );
    Ok(())
}
