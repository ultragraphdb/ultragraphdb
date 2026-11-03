/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{BodyView, Lease, LeaseView};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::lease::lease_tests::{golden_lease, golden_lease_reply};
use crate::utils::golden_bodies::{frame_of, put, reframe, view_any, with_command_header};
use crate::utils::golden_roles::{golden_role, role_bytes};

/// The golden role frame `name`'s bytes.
fn bytes(name: &str) -> TestResult<Vec<u8>> {
    role_bytes(&golden_role(name)?)
}

/// The refusal of `frame` by the LeaseView.
fn refusal(frame: &[u8]) -> TestResult<ProtocolError> {
    LeaseView::from_frame(&frame_of(frame)?)
        .err()
        .context("a refusal")
}

/// The golden frame `name` with its command header changed by `change`, refused.
fn header_refusal(name: &str, change: impl FnOnce(&mut [u8])) -> TestResult<ProtocolError> {
    refusal(&with_command_header(&bytes(name)?, change))
}

#[test]
fn test_the_golden_leases_read_their_values() -> TestResult {
    let request = bytes("lease")?;
    let view = LeaseView::from_frame(&frame_of(&request)?)?;
    assert_eq!(
        (
            view.type_prefix(),
            view.partition(),
            view.size(),
            view.start(),
            view.end()
        ),
        (2, 5, 4096, 0, 0)
    );
    assert!(view.is_request());
    assert_eq!(view.to_body()?, golden_lease());
    let reply = bytes("lease_reply")?;
    let second = LeaseView::from_frame(&frame_of(&reply)?)?;
    assert_eq!(
        (second.start(), second.end(), second.is_request()),
        (81_921, 86_017, false)
    );
    assert_eq!(second.to_body()?, golden_lease_reply());
    assert_eq!(view_any(&request)?, Ok(()));
    assert_eq!(view_any(&reply)?, Ok(()));
    Ok(())
}

#[test]
fn test_unused_bytes_type_partition_and_size_in_order() -> TestResult {
    for at in [2, 3, 32, 127] {
        assert_eq!(
            header_refusal("lease", |h| put(h, at, 1, 1))?,
            ProtocolError::reserved_not_zero(128 + at)
        );
    }
    assert_eq!(
        header_refusal("lease", |h| {
            put(h, 0, 0, 2);
            put(h, 4, 0, 4);
        })?,
        ProtocolError::field_value(516, "type", 0)
    );
    assert_eq!(
        header_refusal("lease", |h| {
            put(h, 4, 0, 4);
            put(h, 8, 0, 8);
        })?,
        ProtocolError::field_value(516, "partition", 0)
    );
    assert_eq!(
        header_refusal("lease", |h| put(h, 8, 0, 8))?,
        ProtocolError::field_value(516, "size", 0)
    );
    Ok(())
}

#[test]
fn test_a_reply_names_a_whole_block() -> TestResult {
    for (start, end) in [
        (0, 4096),
        (1, 4096),
        (1, 4098),
        (81_921, 0),
        (81_921, 81_921),
    ] {
        assert_eq!(
            header_refusal("lease", |h| {
                put(h, 16, start, 8);
                put(h, 24, end, 8);
            })?,
            ProtocolError::field_value(516, "block", start)
        );
    }
    // A block past the last u64 sequence.
    let past = u64::MAX - 4095;
    assert_eq!(
        header_refusal("lease", |h| {
            put(h, 16, past, 8);
            put(h, 24, past.wrapping_add(4096), 8);
        })?,
        ProtocolError::field_value(516, "block", past)
    );
    // The last block a u64 holds, and the extremes of each field.
    let last = u64::MAX - 4096;
    let frame = with_command_header(&bytes("lease")?, |h| {
        put(h, 16, last, 8);
        put(h, 24, u64::MAX, 8);
    });
    let view = LeaseView::from_frame(&frame_of(&frame)?)?;
    assert_eq!(view.to_body()?, Lease::request(2, 5, 4096).with_block(last));
    let most = with_command_header(&bytes("lease")?, |h| {
        put(h, 0, u16::MAX.into(), 2);
        put(h, 4, u32::MAX.into(), 4);
        put(h, 8, 1, 8);
        put(h, 16, 1, 8);
        put(h, 24, 2, 8);
    });
    let second = LeaseView::from_frame(&frame_of(&most)?)?;
    assert_eq!(
        (second.type_prefix(), second.partition(), second.size()),
        (u16::MAX, u32::MAX, 1)
    );
    Ok(())
}

#[test]
fn test_a_lease_has_no_sections() -> TestResult {
    assert_eq!(
        refusal(&reframe(&bytes("lease_reply")?, &[0; 8]))?,
        ProtocolError::trailing_bytes(516, 0, 8)
    );
    Ok(())
}
