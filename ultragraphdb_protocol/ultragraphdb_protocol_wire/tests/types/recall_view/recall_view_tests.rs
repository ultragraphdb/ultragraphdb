/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, Recall, RecallView};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::bind::bind_tests::SESSION;
use crate::utils::golden_bodies::{frame_of, put, view_any, with_command_header};
use crate::utils::golden_roles::{golden_role, role_bytes};

fn refused(change: impl FnOnce(&mut [u8])) -> TestResult<ProtocolErrorEnum> {
    let bytes = with_command_header(&role_bytes(&golden_role("recall")?)?, change);
    Ok(RecallView::from_frame(&frame_of(&bytes)?)
        .err()
        .context("a refusal")?
        .0)
}

fn field(field: &'static str, value: u64) -> ProtocolErrorEnum {
    ProtocolErrorEnum::FieldValue {
        command: 522,
        field,
        value,
    }
}

#[test]
fn test_the_golden_recall_reads_back_to_its_body() -> TestResult {
    let bytes = role_bytes(&golden_role("recall")?)?;
    let view = RecallView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(
        (view.partition(), view.session(), view.request()),
        (3, SESSION, 8)
    );
    assert_eq!(view.to_body()?, Recall::new(3, SESSION, 8));
    view_any(&bytes)??;
    Ok(())
}

#[test]
fn test_the_header_is_checked_in_order() -> TestResult {
    for at in [4usize, 7, 32, 127] {
        assert_eq!(
            refused(|h| {
                h[at] = 1;
                put(h, 0, 0, 4);
            })?,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    assert_eq!(
        refused(|h| {
            put(h, 0, 0, 4);
            h[8..24].fill(0);
        })?,
        field("partition", 0)
    );
    assert_eq!(
        refused(|h| {
            h[8..24].fill(0);
            put(h, 24, 0, 8);
        })?,
        field("session", 0)
    );
    assert_eq!(refused(|h| put(h, 24, 0, 8))?, field("request", 0));
    // A session with one nonzero byte, its last, is a session.
    let bytes = with_command_header(&role_bytes(&golden_role("recall")?)?, |h| {
        h[8..23].fill(0);
        put(h, 24, u64::MAX, 8);
    });
    let view = RecallView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!((view.session()[15], view.request()), (0x20, u64::MAX));
    Ok(())
}
