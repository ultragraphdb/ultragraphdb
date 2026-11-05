/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{
    BodyView, BucketRange, MAX_EPOCH, PlacementView, PreferredLeader, TenantRun,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::placement::placement_tests::{golden_placement, golden_placement_reply};
use crate::utils::golden_bodies::{
    body_data, frame_of, put, section, view_any, with_command_header, with_section,
};
use crate::utils::golden_roles::{golden_role, role_bytes};

/// The golden role frame `name`'s bytes.
fn bytes(name: &str) -> TestResult<Vec<u8>> {
    role_bytes(&golden_role(name)?)
}

/// The refusal of `frame` by the PlacementView.
fn refusal(frame: &[u8]) -> TestResult<ProtocolError> {
    PlacementView::from_frame(&frame_of(frame)?)
        .err()
        .context("a refusal")
}

/// The golden frame `name` with its command header changed by `change`, refused.
fn header_refusal(name: &str, change: impl FnOnce(&mut [u8])) -> TestResult<ProtocolError> {
    refusal(&with_command_header(&bytes(name)?, change))
}

/// The golden reply with section `index` changed by `change`, refused.
fn section_refusal(index: usize, change: impl FnOnce(&mut Vec<u8>)) -> TestResult<ProtocolError> {
    refusal(&with_section(&bytes("placement_reply")?, 3, index, change))
}

/// Writes the u32 `value` at `at`.
fn set(s: &mut [u8], at: usize, value: u32) {
    s[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn test_the_golden_placements_read_their_values() -> TestResult {
    let request = bytes("placement")?;
    let view = PlacementView::from_frame(&frame_of(&request)?)?;
    assert!(view.is_request());
    assert_eq!(
        (view.position(), view.epoch(), view.from(), view.until()),
        (0, 42, 0, 0)
    );
    assert_eq!(
        (view.range_count(), view.run_count(), view.leader_count()),
        (0, 0, 0)
    );
    assert_eq!(view.to_body()?, golden_placement());
    let reply = bytes("placement_reply")?;
    let second = PlacementView::from_frame(&frame_of(&reply)?)?;
    assert!(!second.is_request());
    assert_eq!(
        (
            second.position(),
            second.epoch(),
            second.from(),
            second.until()
        ),
        (1_234, 42, 40, 45)
    );
    assert_eq!(second.range_count(), 3);
    assert_eq!(
        second.range(1),
        Some(BucketRange::new(0x40_0000, 0x80_0000, 2))
    );
    assert_eq!(
        second.range(2),
        Some(BucketRange::new(0x80_0000, 1 << 24, 1))
    );
    assert_eq!(second.range(3), None);
    assert_eq!(second.run_count(), 2);
    assert_eq!(second.run(1), Some(TenantRun::new(7, 0x81)));
    assert_eq!(second.run(2), None);
    assert_eq!(second.leader_count(), 2);
    assert_eq!(second.leader(0), Some(PreferredLeader::new(1, 3)));
    assert_eq!(second.leader(2), None);
    assert_eq!(second.to_body()?, golden_placement_reply());
    assert_eq!(view_any(&request)?, Ok(()));
    assert_eq!(view_any(&reply)?, Ok(()));
    Ok(())
}

#[test]
fn test_unused_bytes_and_epochs_are_checked_first() -> TestResult {
    for at in [32, 127] {
        assert_eq!(
            header_refusal("placement_reply", |h| put(h, at, 1, 1))?,
            ProtocolError::reserved_not_zero(128 + at)
        );
    }
    for (at, field) in [(8, "epoch"), (16, "from"), (24, "until")] {
        assert_eq!(
            header_refusal("placement_reply", |h| put(h, at, MAX_EPOCH + 1, 8))?,
            ProtocolError::field_value(517, field, MAX_EPOCH + 1)
        );
    }
    Ok(())
}

#[test]
fn test_a_request_names_only_its_epoch() -> TestResult {
    assert_eq!(
        header_refusal("placement", |h| put(h, 0, 5, 8))?,
        ProtocolError::field_value(517, "placement", 5)
    );
    assert_eq!(
        header_refusal("placement", |h| put(h, 24, 7, 8))?,
        ProtocolError::field_value(517, "placement", 7)
    );
    let request = bytes("placement")?;
    let with_leader = with_section(&request, 3, 2, |s| *s = vec![1, 0, 0, 0, 1, 0, 0, 0]);
    assert_eq!(
        refusal(&with_leader)?,
        ProtocolError::count_mismatch(517, "placement", 8, 0)
    );
    // A request's position is checked before its sections are read.
    let ragged = with_section(&request, 3, 0, |s| s.push(1));
    assert_eq!(
        refusal(&with_command_header(&ragged, |h| put(h, 0, 5, 8)))?,
        ProtocolError::field_value(517, "placement", 5)
    );
    let latest = with_command_header(&request, |h| put(h, 8, 0, 8));
    assert_eq!(PlacementView::from_frame(&frame_of(&latest)?)?.epoch(), 0);
    Ok(())
}

#[test]
fn test_a_reply_applies_around_its_epoch() -> TestResult {
    // from above the epoch asked for; until not above from, or not above the epoch.
    assert_eq!(
        header_refusal("placement_reply", |h| put(h, 16, 43, 8))?,
        ProtocolError::field_value(517, "placement epoch", 43)
    );
    assert_eq!(
        header_refusal("placement_reply", |h| put(h, 24, 40, 8))?,
        ProtocolError::field_value(517, "placement epoch", 40)
    );
    assert_eq!(
        header_refusal("placement_reply", |h| put(h, 24, 42, 8))?,
        ProtocolError::field_value(517, "placement epoch", 42)
    );
    // Asked for the latest: from any epoch; until above from alone. from equal to the epoch; until 0.
    for (epoch, from, until) in [(0u64, 50u64, 51u64), (42, 42, 0), (42, 1, 43), (0, 1, 0)] {
        let frame = with_command_header(&bytes("placement_reply")?, |h| {
            put(h, 8, epoch, 8);
            put(h, 16, from, 8);
            put(h, 24, until, 8);
        });
        let view = PlacementView::from_frame(&frame_of(&frame)?)?;
        assert_eq!(
            (view.epoch(), view.from(), view.until()),
            (epoch, from, until)
        );
    }
    assert_eq!(
        header_refusal("placement_reply", |h| {
            put(h, 8, 0, 8);
            put(h, 24, 40, 8);
        })?,
        ProtocolError::field_value(517, "placement epoch", 40)
    );
    Ok(())
}

#[test]
fn test_each_section_holds_whole_records() -> TestResult {
    for (index, len, unit) in [(0usize, 49usize, 16usize), (1, 33, 16), (2, 17, 8)] {
        assert_eq!(
            section_refusal(index, |s| s.push(0))?,
            ProtocolError::section_length(517, index, len, unit)
        );
    }
    Ok(())
}

#[test]
fn test_ranges_cover_every_bucket_once_in_order() -> TestResult {
    let start = section(body_data(&bytes("placement_reply")?), 0, 0).0;
    for byte in 12..16 {
        assert_eq!(
            section_refusal(0, |s| s[16 + byte] = 1)?,
            ProtocolError::padding_not_zero(517, start + 16 + byte)
        );
    }
    let cases: [(usize, u32, u64); 6] = [
        (0, 1, 0),          // the first range starting past 0
        (16, 0x40_0001, 1), // a gap
        (16, 0x3F_FFFF, 1), // an overlap
        (20, 0x40_0000, 1), // an empty range
        (24, 0, 1),         // partition 0
        (24, 1, 1),         // the neighbour's partition
    ];
    for (at, value, range) in cases {
        assert_eq!(
            section_refusal(0, |s| set(s, at, value))?,
            ProtocolError::field_value(517, "range", range)
        );
    }
    // The last range ending short of 2^24, and past it; no range at all.
    assert_eq!(
        section_refusal(0, |s| set(s, 36, (1 << 24) - 1))?,
        ProtocolError::field_value(517, "range", 3)
    );
    assert_eq!(
        section_refusal(0, |s| set(s, 36, (1 << 24) + 1))?,
        ProtocolError::field_value(517, "range", 3)
    );
    let none = with_section(
        &with_section(&bytes("placement_reply")?, 3, 0, Vec::clear),
        3,
        2,
        Vec::clear,
    );
    assert_eq!(refusal(&none)?, ProtocolError::field_value(517, "range", 0));
    // One range over every bucket.
    let one = with_section(
        &with_section(&bytes("placement_reply")?, 3, 0, |s| {
            s.truncate(16);
            set(s, 4, 1 << 24);
        }),
        3,
        2,
        |s| s.truncate(8),
    );
    assert_eq!(
        PlacementView::from_frame(&frame_of(&one)?)?.range_count(),
        1
    );
    Ok(())
}

#[test]
fn test_ranges_are_checked_before_runs_and_runs_before_leaders() -> TestResult {
    let golden = bytes("placement_reply")?;
    let both = with_section(&with_section(&golden, 3, 0, |s| set(s, 0, 1)), 3, 1, |s| {
        s[9] = 1
    });
    assert_eq!(refusal(&both)?, ProtocolError::field_value(517, "range", 0));
    let start = section(body_data(&golden), 0, 1).0;
    for byte in 9..16 {
        assert_eq!(
            section_refusal(1, |s| s[16 + byte] = 1)?,
            ProtocolError::padding_not_zero(517, start + 16 + byte)
        );
    }
    for tenant in [3u32, 2] {
        assert_eq!(
            section_refusal(1, |s| set(s, 16, tenant))?,
            ProtocolError::field_value(517, "run", 1)
        );
    }
    let runs_and_leaders =
        with_section(&with_section(&golden, 3, 1, |s| set(s, 16, 3)), 3, 2, |s| {
            set(s, 4, 0)
        });
    assert_eq!(
        refusal(&runs_and_leaders)?,
        ProtocolError::field_value(517, "run", 1)
    );
    Ok(())
}

#[test]
fn test_one_leader_per_partition_the_ranges_name() -> TestResult {
    assert_eq!(
        section_refusal(2, |s| set(s, 4, 0))?,
        ProtocolError::field_value(517, "leader", 0)
    );
    for partition in [1u32, 0] {
        assert_eq!(
            section_refusal(2, |s| set(s, 8, partition))?,
            ProtocolError::field_value(517, "leader", 1)
        );
    }
    assert_eq!(
        section_refusal(2, |s| s.truncate(8))?,
        ProtocolError::count_mismatch(517, "leaders", 1, 2)
    );
    assert_eq!(
        section_refusal(2, |s| set(s, 8, 3))?,
        ProtocolError::count_mismatch(517, "leaders", 2, 2)
    );
    assert_eq!(
        section_refusal(2, |s| s.extend_from_slice(&[3, 0, 0, 0, 1, 0, 0, 0]))?,
        ProtocolError::count_mismatch(517, "leaders", 3, 2)
    );
    Ok(())
}
