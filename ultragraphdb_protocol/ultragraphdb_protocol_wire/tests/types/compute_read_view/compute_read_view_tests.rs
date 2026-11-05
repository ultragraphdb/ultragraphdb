/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, ComputeReadView, MAX_EPOCH, ReadLevel};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::compute_read::compute_read_tests::{golden_compute_read, golden_compute_read_rw};
use crate::utils::golden_bodies::{frame_of, put, view_any, with_command_header, with_section};
use crate::utils::golden_roles::{golden_role, role_bytes};

fn bytes(name: &str) -> TestResult<Vec<u8>> {
    role_bytes(&golden_role(name)?)
}

fn refused(bytes: &[u8]) -> TestResult<ProtocolErrorEnum> {
    Ok(ComputeReadView::from_frame(&frame_of(bytes)?)
        .err()
        .context("a refusal")?
        .0)
}

fn field(field: &'static str, value: u64) -> ProtocolErrorEnum {
    ProtocolErrorEnum::FieldValue {
        command: 523,
        field,
        value,
    }
}

#[test]
fn test_the_golden_reads_read_back_to_their_bodies() -> TestResult {
    let read = bytes("compute_read")?;
    let view = ComputeReadView::from_frame(&frame_of(&read)?)?;
    assert_eq!(
        (view.partition(), view.read_only(), view.level()),
        (0, true, ReadLevel::MonotonicFast)
    );
    assert_eq!(
        (view.named(), view.tenant(), view.read_version()),
        (false, 5, 0)
    );
    assert_eq!(
        (view.epoch(), view.principal(), view.transaction()),
        (31, 9, [0; 16])
    );
    assert_eq!(
        (view.limit(), view.deadline_ms(), view.resume()),
        (100, 50, &b"rk1"[..])
    );
    assert_eq!(view.terms().len(), 2);
    assert_eq!(view.pattern(), golden_compute_read().pattern());
    assert_eq!(view.to_body()?, golden_compute_read());
    let rw = bytes("compute_read_rw")?;
    assert_eq!(
        ComputeReadView::from_frame(&frame_of(&rw)?)?.to_body()?,
        golden_compute_read_rw()
    );
    for frame in [read, rw] {
        view_any(&frame)??;
    }
    Ok(())
}

#[test]
fn test_unused_bytes_mode_and_level_come_first() -> TestResult {
    let read = bytes("compute_read")?;
    for at in [7usize, 64, 127] {
        let bad = with_command_header(&read, |h| {
            h[at] = 1;
            h[4] = 2;
        });
        assert_eq!(
            refused(&bad)?,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    let bad_2 = with_command_header(&read, |h| {
        h[4] = 2;
        h[5] = 3;
    });
    assert_eq!(refused(&bad_2)?, field("mode", 2));
    let bad_3 = with_command_header(&read, |h| {
        h[5] = 3;
        h[6] = 2;
    });
    assert_eq!(refused(&bad_3)?, field("level", 3));
    Ok(())
}

#[test]
fn test_only_a_read_only_snapshot_names_its_version() -> TestResult {
    let read = bytes("compute_read")?;
    let bad = with_command_header(&read, |h| {
        h[6] = 2;
        put(h, 24, MAX_EPOCH + 1, 8);
    });
    assert_eq!(refused(&bad)?, field("named", 2));
    // Level 2, and a read-write transaction at level 0.
    let bad_2 = with_command_header(&read, |h| h[6] = 1);
    assert_eq!(refused(&bad_2)?, field("named", 1));
    let bad_3 = with_command_header(&bytes("compute_read_rw")?, |h| {
        h[5] = 0;
        h[6] = 1;
    });
    assert_eq!(refused(&bad_3)?, field("named", 1));
    let named = with_command_header(&read, |h| {
        h[5] = 0;
        h[6] = 1;
        put(h, 16, 77, 8);
    });
    let view = ComputeReadView::from_frame(&frame_of(&named)?)?;
    assert_eq!(
        (view.named(), view.level(), view.read_version()),
        (true, ReadLevel::Snapshot, 77)
    );
    let body = view.to_body()?;
    assert_eq!(
        (body.named(), body.level(), body.read_version()),
        (true, ReadLevel::Snapshot, 77)
    );
    Ok(())
}

#[test]
fn test_the_epoch_then_the_transaction_are_checked() -> TestResult {
    let read = bytes("compute_read")?;
    let bad = with_command_header(&read, |h| {
        put(h, 24, MAX_EPOCH + 1, 8);
        h[40] = 1;
    });
    assert_eq!(refused(&bad)?, field("epoch", MAX_EPOCH + 1));
    let most = with_command_header(&read, |h| put(h, 24, MAX_EPOCH, 8));
    assert_eq!(
        ComputeReadView::from_frame(&frame_of(&most)?)?.epoch(),
        MAX_EPOCH
    );
    let bad_2 = with_command_header(&read, |h| h[55] = 1);
    assert_eq!(refused(&bad_2)?, field("transaction", 1));
    let rw = bytes("compute_read_rw")?;
    let bad_3 = with_command_header(&rw, |h| put(h, 0, 0, 4));
    assert_eq!(refused(&bad_3)?, field("transaction", 0));
    let bad_4 = with_command_header(&rw, |h| h[40..56].fill(0));
    assert_eq!(refused(&bad_4)?, field("transaction", 0));
    Ok(())
}

#[test]
fn test_the_term_table_comes_before_the_pattern_which_takes_no_parameter() -> TestResult {
    let read = bytes("compute_read")?;
    let bad = with_section(&read, 4, 0, |p| put(p, 0, 3, 4));
    assert_eq!(refused(&bad)?, field("parameter slot", 0));
    let bad_2 = with_section(&bad, 4, 1, |s: &mut Vec<u8>| {
        s.truncate(s.len().saturating_sub(1))
    });
    assert_eq!(
        refused(&bad_2)?,
        ProtocolErrorEnum::SectionLength {
            command: 523,
            section: 1,
            len: 47,
            unit: 24
        }
    );
    Ok(())
}
