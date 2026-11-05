/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{SCAN_RANGE_SIZE, ScanRange};
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_a_range_encodes_its_partition_and_key_offsets() -> TestResult {
    let range = ScanRange::new(0x0102_0304, b"\x10abc", b"\x10ab");
    assert_eq!(range.partition(), 0x0102_0304);
    assert_eq!(range.first(), b"\x10abc");
    assert_eq!(range.past(), b"\x10ab");
    let mut expected = [0u8; SCAN_RANGE_SIZE];
    expected[0..4].copy_from_slice(&0x0102_0304u32.to_le_bytes());
    expected[8..12].copy_from_slice(&7u32.to_le_bytes());
    expected[12..16].copy_from_slice(&4u32.to_le_bytes());
    expected[16..20].copy_from_slice(&11u32.to_le_bytes());
    expected[20..24].copy_from_slice(&3u32.to_le_bytes());
    assert_eq!(range.encode(7), expected);
    // The end of the first key's keyspace: an empty past key, its offset still the first key's end.
    let open = ScanRange::new(1, b"k", b"").encode(0);
    assert_eq!(&open[16..24], &[1, 0, 0, 0, 0, 0, 0, 0]);
    Ok(())
}

#[test]
fn test_a_range_with_an_after_states_its_length() {
    let range = ScanRange::new(1, b"\x10a", b"\x10b").with_after(b"object");
    assert_eq!(range.after(), b"object");
    let record = range.encode(0);
    assert_eq!(&record[4..8], &6u32.to_le_bytes());
    assert!(ScanRange::new(1, b"\x10a", b"").after().is_empty());
}
