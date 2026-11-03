/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{BodyView, MAX_EPOCH, Subscribe, SubscribeView};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::subscribe::subscribe_tests::golden_subscribe;
use crate::utils::golden_bodies::{frame_of, put, reframe, view_any, with_command_header};
use crate::utils::golden_roles::{golden_role, role_bytes};

/// The golden role frame `name`'s bytes.
fn bytes(name: &str) -> TestResult<Vec<u8>> {
    role_bytes(&golden_role(name)?)
}

/// The refusal of `frame` by the SubscribeView.
fn refusal(frame: &[u8]) -> TestResult<ProtocolError> {
    SubscribeView::from_frame(&frame_of(frame)?)
        .err()
        .context("a refusal")
}

/// The golden frame `name` with its command header changed by `change`, refused.
fn header_refusal(name: &str, change: impl FnOnce(&mut [u8])) -> TestResult<ProtocolError> {
    refusal(&with_command_header(&bytes(name)?, change))
}

#[test]
fn test_the_golden_subscribe_reads_its_epoch() -> TestResult {
    let golden = bytes("subscribe")?;
    let view = SubscribeView::from_frame(&frame_of(&golden)?)?;
    assert_eq!(view.from(), 41);
    assert_eq!(view.to_body()?, golden_subscribe());
    assert_eq!(view_any(&golden)?, Ok(()));
    for from in [0, MAX_EPOCH] {
        let frame = with_command_header(&golden, |h| put(h, 0, from, 8));
        let second = SubscribeView::from_frame(&frame_of(&frame)?)?;
        assert_eq!(second.to_body()?, Subscribe::new(from));
    }
    Ok(())
}

#[test]
fn test_unused_bytes_the_epoch_and_the_sections_are_checked() -> TestResult {
    assert_eq!(
        header_refusal("subscribe", |h| put(h, 8, 1, 1))?,
        ProtocolError::reserved_not_zero(136)
    );
    for from in [MAX_EPOCH + 1, u64::MAX] {
        assert_eq!(
            header_refusal("subscribe", |h| put(h, 0, from, 8))?,
            ProtocolError::field_value(514, "from", from)
        );
    }
    let golden = bytes("subscribe")?;
    assert_eq!(
        refusal(&reframe(&golden, &[0; 8]))?,
        ProtocolError::trailing_bytes(514, 0, 8)
    );
    Ok(())
}
