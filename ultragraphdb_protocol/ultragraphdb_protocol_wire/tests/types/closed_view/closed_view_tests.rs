/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, Closed, ClosedPartition, ClosedView, MAX_EPOCH};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_bodies::{frame_of, view_any, with_command_header, with_section};
use crate::utils::golden_roles::{golden_role, role_bytes};
use crate::utils::term_records::refusal;

fn golden(name: &str) -> TestResult<Vec<u8>> {
    role_bytes(&golden_role(name)?)
}

/// The golden request with its partitions replaced by `records`, each partition, its zero bytes and epoch.
fn with_records(records: &[(u32, u32, u64)]) -> TestResult<Vec<u8>> {
    Ok(with_section(&golden("closed")?, 1, 0, |s| {
        *s = records
            .iter()
            .flat_map(|(p, zero, e)| {
                p.to_le_bytes()
                    .into_iter()
                    .chain(zero.to_le_bytes())
                    .chain(e.to_le_bytes())
            })
            .collect();
    }))
}

fn field(field: &'static str, value: u64) -> ProtocolErrorEnum {
    ProtocolErrorEnum::FieldValue {
        command: 528,
        field,
        value,
    }
}

#[test]
fn test_the_golden_request_and_reply_read() -> TestResult {
    let request = golden("closed")?;
    let view = ClosedView::from_frame(&frame_of(&request)?)?;
    assert_eq!(view.len(), 3);
    assert!(!view.is_empty());
    assert_eq!(view.partition(1), Some(ClosedPartition::new(4, 0)));
    assert_eq!(view.partition(3), None);
    assert_eq!(view.check_request(), Ok(()));
    assert_eq!(view.to_body()?, Closed::request(&[2, 4, 9]));
    let reply = golden("closed_reply")?;
    let reply_view = ClosedView::from_frame(&frame_of(&reply)?)?;
    let partitions: Vec<ClosedPartition> = reply_view.partitions().collect();
    assert_eq!(
        partitions,
        vec![
            ClosedPartition::new(2, 28),
            ClosedPartition::new(4, 0),
            ClosedPartition::new(9, 31)
        ]
    );
    for bytes in [request, reply] {
        assert_eq!(view_any(&bytes)?, Ok(()));
    }
    Ok(())
}

#[test]
fn test_a_reply_is_not_a_request() -> TestResult {
    let reply = golden("closed_reply")?;
    let view = ClosedView::from_frame(&frame_of(&reply)?)?;
    assert_eq!(
        view.check_request().map_err(|e| e.0),
        Err(field("epoch", 28))
    );
    let last = with_records(&[(2, 0, 0), (4, 0, 0), (9, 0, 1)])?;
    let last_view = ClosedView::from_frame(&frame_of(&last)?)?;
    assert_eq!(
        last_view.check_request().map_err(|e| e.0),
        Err(field("epoch", 1))
    );
    Ok(())
}

#[test]
fn test_the_command_header_is_all_zero() -> TestResult {
    for at in [0usize, 127] {
        let bytes = with_command_header(&golden("closed")?, |h| h[at] = 1);
        assert_eq!(
            refusal::<ClosedView>(&bytes)?,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    Ok(())
}

#[test]
fn test_the_partitions_are_whole_and_at_least_one() -> TestResult {
    let empty = with_records(&[])?;
    assert_eq!(
        refusal::<ClosedView>(&empty)?,
        ProtocolErrorEnum::CountMismatch {
            command: 528,
            field: "partitions",
            count: 0,
            expected: 1
        }
    );
    let torn = with_section(&golden("closed")?, 1, 0, |s| s.truncate(31));
    assert_eq!(
        refusal::<ClosedView>(&torn)?,
        ProtocolErrorEnum::SectionLength {
            command: 528,
            section: 0,
            len: 31,
            unit: 16
        }
    );
    Ok(())
}

#[test]
fn test_zero_bytes_are_named_at_their_body_offset() -> TestResult {
    for (zero, offset) in [(1u32, 4usize), (1 << 24, 7)] {
        let bytes = with_records(&[(2, 0, 0), (4, zero, 0)])?;
        // The section starts at body offset 8; partition 1's zero bytes at 16 + 4.
        assert_eq!(
            refusal::<ClosedView>(&bytes)?,
            ProtocolErrorEnum::PaddingNotZero {
                command: 528,
                offset: 8 + 16 + offset
            }
        );
    }
    Ok(())
}

#[test]
fn test_partitions_ascend_from_one() -> TestResult {
    for (records, value) in [
        (vec![(0u32, 0u32, 0u64)], 0u64),
        (vec![(2, 0, 0), (2, 0, 0)], 2),
        (vec![(5, 0, 0), (3, 0, 0)], 3),
    ] {
        let bytes = with_records(&records)?;
        assert_eq!(refusal::<ClosedView>(&bytes)?, field("partition", value));
    }
    let bytes = with_records(&[(1, 0, 0), (u32::MAX, 0, MAX_EPOCH)])?;
    assert_eq!(ClosedView::from_frame(&frame_of(&bytes)?)?.len(), 2);
    Ok(())
}

#[test]
fn test_an_epoch_past_42_bits_is_refused() -> TestResult {
    let bytes = with_records(&[(2, 0, 0), (4, 0, MAX_EPOCH + 1)])?;
    assert_eq!(
        refusal::<ClosedView>(&bytes)?,
        field("epoch", MAX_EPOCH + 1)
    );
    let order = with_records(&[(2, 0, MAX_EPOCH + 1), (1, 0, 0)])?;
    assert_eq!(
        refusal::<ClosedView>(&order)?,
        field("epoch", MAX_EPOCH + 1)
    );
    Ok(())
}
