/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Epoch, SessionToken};
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{Begin, BeginView, BodyView, ReadLevel};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::golden_bodies::{frame_of, golden, with_command_header};

#[test]
fn test_the_golden_begin_reads_its_values() -> TestResult {
    let bytes = golden("begin")?.bytes()?;
    let view = BeginView::from_frame(&frame_of(&bytes)?)?;
    let token = SessionToken::new(2, 41, Epoch::new(30)?);
    assert_eq!(view.tenant(), 3);
    assert_eq!(view.read_version(), 0);
    assert!(view.read_only());
    assert_eq!(
        (view.level(), view.deadline_ms()),
        (ReadLevel::MonotonicFast, 50)
    );
    assert_eq!(view.token(), Some(&token));
    assert_eq!(
        view.to_body()?,
        Begin::new(3, 0, true)
            .with_level(ReadLevel::MonotonicFast)
            .with_deadline_ms(50)
            .with_token(token)
    );
    Ok(())
}

#[test]
fn test_a_mode_past_read_only_is_refused() -> TestResult {
    let bytes = golden("begin")?.bytes()?;
    for mode in [2u8, 0x80, 0xFF] {
        let changed = with_command_header(&bytes, |h| h[16] = mode);
        assert_eq!(
            BeginView::from_frame(&frame_of(&changed)?)
                .err()
                .context("a refusal")?
                .0,
            ProtocolErrorEnum::FieldValue {
                command: 4,
                field: "mode",
                value: u64::from(mode)
            }
        );
    }
    let read_write = with_command_header(&bytes, |h| h[16] = 0);
    assert!(!BeginView::from_frame(&frame_of(&read_write)?)?.read_only());
    Ok(())
}

#[test]
fn test_a_read_version_is_a_version_or_a_version_bound() -> TestResult {
    let bytes = golden("begin_named")?.bytes()?;
    for odd in [1u64, (29 << 22) | 7, (29 << 22) | 0x3F_FFFD] {
        let frame = with_command_header(&bytes, |h| {
            h[8..16].copy_from_slice(&odd.to_le_bytes());
        });
        assert_eq!(
            BeginView::from_frame(&frame_of(&frame)?)
                .err()
                .context("a refusal")?
                .0,
            ProtocolErrorEnum::FieldValue {
                command: 4,
                field: "read_version",
                value: odd
            }
        );
    }
    let even: u64 = (29 << 22) | 4;
    let frame = with_command_header(&bytes, |h| {
        h[8..16].copy_from_slice(&even.to_le_bytes());
    });
    assert_eq!(
        BeginView::from_frame(&frame_of(&frame)?)?.read_version(),
        even
    );
    Ok(())
}
