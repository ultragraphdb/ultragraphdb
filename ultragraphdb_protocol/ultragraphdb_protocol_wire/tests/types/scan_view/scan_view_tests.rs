/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, MAX_EPOCH, ScanAt, ScanRange, ScanView};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::scan::scan_tests::{golden_scan, golden_scan_transaction};
use crate::utils::golden_bodies::{frame_of, put, view_any, with_command_header, with_section};
use crate::utils::golden_roles::{golden_role, role_bytes};

fn bytes(name: &str) -> TestResult<Vec<u8>> {
    role_bytes(&golden_role(name)?)
}

fn refused(bytes: &[u8]) -> TestResult<ProtocolErrorEnum> {
    Ok(ScanView::from_frame(&frame_of(bytes)?)
        .err()
        .context("a refusal")?
        .0)
}

fn field(field: &'static str, value: u64) -> ProtocolErrorEnum {
    ProtocolErrorEnum::FieldValue {
        command: 524,
        field,
        value,
    }
}

/// The golden Scan with its ranges' section changed by `change`.
fn ranges(change: impl FnOnce(&mut Vec<u8>)) -> TestResult<Vec<u8>> {
    Ok(with_section(&bytes("scan")?, 2, 0, change))
}

/// The golden Scan with its keys' section changed by `change`.
fn keys(change: impl FnOnce(&mut Vec<u8>)) -> TestResult<Vec<u8>> {
    Ok(with_section(&bytes("scan")?, 2, 1, change))
}

#[test]
fn test_the_golden_scans_read_back_to_their_bodies() -> TestResult {
    let scan = bytes("scan")?;
    let view = ScanView::from_frame(&frame_of(&scan)?)?;
    assert_eq!(
        (view.at(), view.read_version(), view.epoch()),
        (ScanAt::ReadVersion, (30 << 22) | 0x3F_FFFF, 30)
    );
    assert_eq!(
        (view.principal(), view.transaction(), view.limit()),
        (9, [0; 16], 1_000)
    );
    assert_eq!(view.range_count(), 2);
    assert_eq!(view.range(1), Some(ScanRange::new(4, b"\x20k", b"")));
    assert_eq!(view.range(2), None);
    assert_eq!(view.ranges().count(), 2);
    assert_eq!(view.to_body()?, golden_scan());
    let transaction = bytes("scan_transaction")?;
    assert_eq!(
        ScanView::from_frame(&frame_of(&transaction)?)?.to_body()?,
        golden_scan_transaction()
    );
    for frame in [scan, transaction] {
        view_any(&frame)??;
    }
    Ok(())
}

#[test]
fn test_unused_bytes_and_at_come_first() -> TestResult {
    let scan = bytes("scan")?;
    for at in [1usize, 7, 52, 127] {
        let bad = with_command_header(&scan, |h| {
            h[at] = 1;
            h[0] = 0;
        });
        assert_eq!(
            refused(&bad)?,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    for choice in [0u8, 5, 255] {
        let bad_2 = with_command_header(&scan, |h| h[0] = choice);
        assert_eq!(refused(&bad_2)?, field("at", choice.into()));
    }
    Ok(())
}

#[test]
fn test_only_at_1_and_4_name_a_version_and_at_4_waits_for_no_epoch() -> TestResult {
    let scan = bytes("scan")?;
    for choice in [2u8, 3] {
        let bad = with_command_header(&scan, |h| {
            h[0] = choice;
            put(h, 16, MAX_EPOCH + 1, 8);
        });
        assert_eq!(
            refused(&bad)?,
            field("read_version", (30 << 22) | 0x3F_FFFF)
        );
        let latest = with_command_header(&scan, |h| {
            h[0] = choice;
            put(h, 8, 0, 8);
        });
        assert_eq!(ScanView::from_frame(&frame_of(&latest)?)?.read_version(), 0);
    }
    let bad_2 = with_command_header(&scan, |h| h[0] = 4);
    assert_eq!(refused(&bad_2)?, field("epoch", 30));
    let named = with_command_header(&scan, |h| {
        h[0] = 4;
        put(h, 16, 0, 8);
    });
    assert_eq!(
        ScanView::from_frame(&frame_of(&named)?)?.at(),
        ScanAt::Named
    );
    let bad_3 = with_command_header(&scan, |h| put(h, 16, MAX_EPOCH + 1, 8));
    assert_eq!(refused(&bad_3)?, field("epoch", MAX_EPOCH + 1));
    Ok(())
}

#[test]
fn test_the_ranges_are_whole_records_and_at_least_one() -> TestResult {
    assert_eq!(
        refused(&ranges(
            |s: &mut Vec<u8>| s.truncate(s.len().saturating_sub(1))
        )?)?,
        ProtocolErrorEnum::SectionLength {
            command: 524,
            section: 0,
            len: 47,
            unit: 24
        }
    );
    let none = with_section(&ranges(Vec::clear)?, 2, 1, Vec::clear);
    assert_eq!(
        refused(&none)?,
        ProtocolErrorEnum::CountMismatch {
            command: 524,
            field: "ranges",
            count: 0,
            expected: 1
        }
    );
    Ok(())
}

#[test]
fn test_a_range_holds_zero_padding_and_a_partition() -> TestResult {
    // Section 0 starts at body offset 16, after the two-section table; range 0's padding is bytes 4 to 7.
    for j in [0usize, 3] {
        let bad = ranges(|r| {
            r[4 + j] = 1;
            put(r, 0, 0, 4);
        })?;
        assert_eq!(
            refused(&bad)?,
            ProtocolErrorEnum::PaddingNotZero {
                command: 524,
                offset: 20 + j
            }
        );
    }
    // Range 1's padding, at body offset 16 + 24 + 4.
    let second = ranges(|r| r[29] = 1)?;
    assert_eq!(
        refused(&second)?,
        ProtocolErrorEnum::PaddingNotZero {
            command: 524,
            offset: 45
        }
    );
    let bad_2 = ranges(|r| {
        put(r, 24, 0, 4);
        put(r, 32, 9, 4);
    })?;
    assert_eq!(refused(&bad_2)?, field("partition", 0));
    // A transaction's Scan reads one partition: range 1's partition 4 differs from range 0's.
    let bad_3 = with_command_header(&bytes("scan")?, |h| h[32] = 1);
    assert_eq!(refused(&bad_3)?, field("partition", 4));
    Ok(())
}

#[test]
fn test_keys_follow_each_other_without_a_gap() -> TestResult {
    // Range 0's first key at 1, its past key at 5, its first key empty; range 1's first key at 9.
    assert_eq!(refused(&ranges(|r| put(r, 8, 1, 4))?)?, field("range", 0));
    assert_eq!(refused(&ranges(|r| put(r, 16, 5, 4))?)?, field("range", 0));
    let empty = ranges(|r| {
        put(r, 12, 0, 4);
        put(r, 16, 0, 4);
    })?;
    assert_eq!(refused(&empty)?, field("range", 0));
    assert_eq!(refused(&ranges(|r| put(r, 32, 9, 4))?)?, field("range", 1));
    Ok(())
}

#[test]
fn test_keys_lie_inside_their_section_and_fill_it() -> TestResult {
    // Range 1's past key of 3 bytes ends at 13, past the 10 bytes of keys.
    assert_eq!(
        refused(&ranges(|r| put(r, 44, 3, 4))?)?,
        ProtocolErrorEnum::IndexOutOfRange {
            command: 524,
            section: 0,
            at: 1,
            index: 13,
            limit: 10
        }
    );
    assert_eq!(
        refused(&keys(|k| k.push(0))?)?,
        ProtocolErrorEnum::CountMismatch {
            command: 524,
            field: "range keys",
            count: 11,
            expected: 10
        }
    );
    Ok(())
}

#[test]
fn test_a_past_key_lies_above_its_first() -> TestResult {
    // Range 0's past key, bytes 4 to 7, made equal to its first and then below it.
    assert_eq!(refused(&keys(|k| k[7] = b'c')?)?, field("range", 0));
    assert_eq!(refused(&keys(|k| k[7] = b'b')?)?, field("range", 0));
    // A past key longer than its first, sharing it as a prefix, lies above it.
    let longer = ranges(|r| {
        put(r, 20, 5, 4);
        put(r, 32, 9, 4);
        put(r, 40, 11, 4);
    })
    .map(|frame| with_section(&frame, 2, 1, |k| k.insert(8, 0)))?;
    let view = ScanView::from_frame(&frame_of(&longer)?)?;
    assert_eq!(view.range(0).map(|r| r.past().len()), Some(5));
    Ok(())
}
