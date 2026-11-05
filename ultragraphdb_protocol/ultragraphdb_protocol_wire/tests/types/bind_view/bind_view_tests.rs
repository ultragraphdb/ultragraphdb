/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{Bind, BindKind, BindView, BodyView, MAX_EPOCH};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::bind::bind_tests::{SESSION, TRANSACTION, WATERMARK, golden_bind};
use crate::utils::golden_bodies::{
    body_data, frame_of, put, reframe, view_any, with_command_header,
};
use crate::utils::golden_roles::{golden_role, role_bytes};

fn bytes(name: &str) -> TestResult<Vec<u8>> {
    role_bytes(&golden_role(name)?)
}

fn refused(bytes: &[u8]) -> TestResult<ProtocolErrorEnum> {
    Ok(BindView::from_frame(&frame_of(bytes)?)
        .err()
        .context("a refusal")?
        .0)
}

fn field(field: &'static str, value: u64) -> ProtocolErrorEnum {
    ProtocolErrorEnum::FieldValue {
        command: 521,
        field,
        value,
    }
}

#[test]
fn test_the_golden_binds_read_back_to_their_bodies() -> TestResult {
    let reply = bytes("bind_reply")?;
    let view = BindView::from_frame(&frame_of(&reply)?)?;
    assert_eq!(
        (view.partition(), view.kind(), view.tenant()),
        (3, BindKind::Transaction, 5)
    );
    assert_eq!(
        (view.read_version(), view.epoch(), view.principal()),
        (WATERMARK, 30, 9)
    );
    assert_eq!(view.transaction(), TRANSACTION);
    assert_eq!(view.to_body()?, golden_bind().with_read_version(WATERMARK));
    let drop = bytes("bind_drop")?;
    let view_2 = BindView::from_frame(&frame_of(&drop)?)?;
    assert_eq!(
        view_2.to_body()?,
        Bind::new(4, BindKind::Drop, 6, 0, 11, SESSION)
    );
    for frame in [bytes("bind")?, reply, drop] {
        view_any(&frame)??;
    }
    Ok(())
}

#[test]
fn test_the_header_is_checked_in_order() -> TestResult {
    let bind = bytes("bind")?;
    for at in [5usize, 7, 56, 127] {
        let bad = with_command_header(&bind, |h| {
            h[at] = 1;
            put(h, 0, 0, 4);
        });
        assert_eq!(
            refused(&bad)?,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    let bad_2 = with_command_header(&bind, |h| {
        put(h, 0, 0, 4);
        h[4] = 0;
    });
    assert_eq!(refused(&bad_2)?, field("partition", 0));
    for kind in [0u8, 3, 255] {
        let bad_3 = with_command_header(&bind, |h| {
            h[4] = kind;
            put(h, 24, MAX_EPOCH + 1, 8);
        });
        assert_eq!(refused(&bad_3)?, field("kind", kind.into()));
    }
    // A Drop's binding reads at no version.
    let bad_4 = with_command_header(&bytes("bind_reply")?, |h| {
        h[4] = 2;
        put(h, 24, MAX_EPOCH + 1, 8);
    });
    assert_eq!(refused(&bad_4)?, field("read_version", WATERMARK));
    let bad_5 = with_command_header(&bind, |h| {
        put(h, 24, MAX_EPOCH + 1, 8);
        h[40..56].fill(0);
    });
    assert_eq!(refused(&bad_5)?, field("epoch", MAX_EPOCH + 1));
    let bad_6 = with_command_header(&bind, |h| h[40..56].fill(0));
    assert_eq!(refused(&bad_6)?, field("transaction", 0));
    Ok(())
}

#[test]
fn test_the_largest_epoch_is_taken_and_no_section_follows() -> TestResult {
    let bind = bytes("bind")?;
    let most = with_command_header(&bind, |h| put(h, 24, MAX_EPOCH, 8));
    assert_eq!(BindView::from_frame(&frame_of(&most)?)?.epoch(), MAX_EPOCH);
    let mut data = body_data(&bind).to_vec();
    data.extend_from_slice(&[0; 8]);
    assert_eq!(
        refused(&reframe(&bind, &data))?,
        ProtocolErrorEnum::TrailingBytes {
            command: 521,
            end: 0,
            len: 8
        }
    );
    Ok(())
}
