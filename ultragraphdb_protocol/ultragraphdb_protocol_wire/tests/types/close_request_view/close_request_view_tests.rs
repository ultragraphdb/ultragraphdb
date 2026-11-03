/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, CloseRequest, CloseRequestView, MAX_EPOCH};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_bodies::{frame_of, put, reframe, view_any, with_command_header};
use crate::utils::golden_roles::{golden_role, role_bytes};
use crate::utils::term_records::refusal;

fn golden() -> TestResult<Vec<u8>> {
    role_bytes(&golden_role("close_request")?)
}

fn field(field: &'static str, value: u64) -> ProtocolErrorEnum {
    ProtocolErrorEnum::FieldValue {
        command: 527,
        field,
        value,
    }
}

#[test]
fn test_the_golden_close_request_reads() -> TestResult {
    let bytes = golden()?;
    let view = CloseRequestView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!((view.partition(), view.epoch()), (7, 30));
    assert_eq!(view.to_body()?, CloseRequest::new(7, 30));
    assert_eq!(view_any(&bytes)?, Ok(()));
    Ok(())
}

#[test]
fn test_unused_header_bytes_are_refused() -> TestResult {
    for at in [4usize, 7, 16, 127] {
        let bytes = with_command_header(&golden()?, |h| h[at] = 1);
        assert_eq!(
            refusal::<CloseRequestView>(&bytes)?,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    Ok(())
}

#[test]
fn test_partition_zero_is_refused_before_the_epoch() -> TestResult {
    let bytes = with_command_header(&golden()?, |h| {
        put(h, 0, 0, 4);
        put(h, 8, 0, 8);
    });
    assert_eq!(refusal::<CloseRequestView>(&bytes)?, field("partition", 0));
    let most = with_command_header(&golden()?, |h| put(h, 0, u32::MAX.into(), 4));
    assert_eq!(
        CloseRequestView::from_frame(&frame_of(&most)?)?.partition(),
        u32::MAX
    );
    Ok(())
}

#[test]
fn test_an_epoch_of_none_or_past_42_bits_is_refused() -> TestResult {
    for epoch in [0, MAX_EPOCH + 1, u64::MAX] {
        let bytes = with_command_header(&golden()?, |h| put(h, 8, epoch, 8));
        assert_eq!(refusal::<CloseRequestView>(&bytes)?, field("epoch", epoch));
    }
    for epoch in [1, MAX_EPOCH] {
        let bytes = with_command_header(&golden()?, |h| put(h, 8, epoch, 8));
        assert_eq!(
            CloseRequestView::from_frame(&frame_of(&bytes)?)?.epoch(),
            epoch
        );
    }
    Ok(())
}

#[test]
fn test_a_body_is_refused() -> TestResult {
    let bytes = reframe(&golden()?, &[0; 8]);
    assert_eq!(
        refusal::<CloseRequestView>(&bytes)?,
        ProtocolErrorEnum::TrailingBytes {
            command: 527,
            end: 0,
            len: 8
        }
    );
    Ok(())
}
