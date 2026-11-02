/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{Begin, BeginView, BodyView};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::golden_bodies::{frame_of, golden, with_command_header};

#[test]
fn test_the_golden_begin_reads_its_values() -> TestResult {
    let bytes = golden("begin")?.bytes()?;
    let view = BeginView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(view.namespace(), 3);
    assert_eq!(view.read_version(), 0xAB_CDEF_0002);
    assert!(view.read_only());
    assert_eq!(view.to_body()?, Begin::new(3, 0xAB_CDEF_0002, true));
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
