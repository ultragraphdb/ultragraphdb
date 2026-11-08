/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, ComputeReadView, MAX_EPOCH, ReadLevel};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::compute_read::compute_read_tests::{golden_compute_read, golden_compute_read_rw};
use crate::utils::golden_bodies::{
    frame_of, put, reframe, view_any, with_command_header, with_section,
};
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
    for at in [7usize, 72, 127] {
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
        put(h, 16, 78, 8);
    });
    let view = ComputeReadView::from_frame(&frame_of(&named)?)?;
    assert_eq!(
        (view.named(), view.level(), view.read_version()),
        (true, ReadLevel::Snapshot, 78)
    );
    let body = view.to_body()?;
    assert_eq!(
        (body.named(), body.level(), body.read_version()),
        (true, ReadLevel::Snapshot, 78)
    );
    Ok(())
}

/// `read_version` is a version or a version bound (`PROTOCOL.md` §1), checked after `named` and before `epoch`.
#[test]
fn test_the_read_version_is_a_version_before_the_epoch() -> TestResult {
    let rw = bytes("compute_read_rw")?;
    let odd = (5u64 << 22) | 3;
    let bad = with_command_header(&rw, |h| {
        put(h, 16, odd, 8);
        put(h, 24, MAX_EPOCH + 1, 8);
    });
    assert_eq!(refused(&bad)?, field("read_version", odd));
    let bad_named = with_command_header(&bad, |h| h[6] = 1);
    assert_eq!(refused(&bad_named)?, field("named", 1));
    for held in [(5u64 << 22) | 2, (5u64 << 22) | 0x3F_FFFF] {
        let good = with_command_header(&rw, |h| put(h, 16, held, 8));
        assert_eq!(
            ComputeReadView::from_frame(&frame_of(&good)?)?.read_version(),
            held
        );
    }
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

#[test]
fn test_the_client_limits_are_read_and_bounded() -> TestResult {
    let read = bytes("compute_read")?;
    let view = ComputeReadView::from_frame(&frame_of(&read)?)?;
    assert_eq!((view.max_frame(), view.credit()), (1 << 20, 4));
    for max_frame in [0u64, (1 << 19) - 1, (1 << 20) + 1] {
        let frame = with_command_header(&read, |h| put(h, 64, max_frame, 4));
        assert_eq!(refused(&frame)?, field("max_frame", max_frame));
    }
    let no_credit = with_command_header(&read, |h| put(h, 68, 0, 4));
    assert_eq!(refused(&no_credit)?, field("credit", 0));
    for max_frame in [1u64 << 19, 1 << 20] {
        let bound = with_command_header(&read, |h| put(h, 64, max_frame, 4));
        assert_eq!(
            u64::from(ComputeReadView::from_frame(&frame_of(&bound)?)?.max_frame()),
            max_frame
        );
    }
    Ok(())
}

#[test]
fn test_the_client_limits_follow_the_transaction_and_come_before_the_sections() -> TestResult {
    let read = bytes("compute_read")?;
    let mode = with_command_header(&read, |h| {
        h[4] = 2;
        put(h, 64, 0, 4);
    });
    assert_eq!(refused(&mode)?, field("mode", 2));
    let frame = with_command_header(&read, |h| {
        put(h, 64, 0, 4);
        put(h, 68, 0, 4);
    });
    assert_eq!(refused(&frame)?, field("max_frame", 0));
    let cut = reframe(&with_command_header(&read, |h| put(h, 68, 0, 4)), &[]);
    assert_eq!(refused(&cut)?, field("credit", 0));
    Ok(())
}

/// A resume key's pattern hash is XXH3-64, seed 0, of the body's sections 0 to 2 (`PROTOCOL.md` §11.8): from section 0's
/// offset to section 2's end, as the section table gives them, hashed with xxhash-rust.
#[test]
fn test_the_pattern_hash_is_xxh3_of_sections_0_to_2() -> TestResult {
    use crate::utils::golden_bodies::{body_data, section};

    for name in ["compute_read", "compute_read_rw"] {
        let read = bytes(name)?;
        let data = body_data(&read);
        let (start, _) = section(data, 0, 0);
        let (offset, len) = section(data, 0, 2);
        let expected = xxhash_rust::xxh3::xxh3_64(&data[start..offset + len]);
        let view = ComputeReadView::from_frame(&frame_of(&read)?)?;
        assert_eq!(view.pattern_hash(), expected, "{name}");
    }
    Ok(())
}
