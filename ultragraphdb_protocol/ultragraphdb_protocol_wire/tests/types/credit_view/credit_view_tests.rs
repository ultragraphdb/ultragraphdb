/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, Credit, CreditView};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::golden_bodies::{frame_of, golden, put, with_command_header};

#[test]
fn test_the_golden_credit_reads_its_frames() -> TestResult {
    let bytes = golden("credit")?.bytes()?;
    let view = CreditView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(view.frames(), 2);
    assert_eq!(view.to_body()?, Credit::new(2));
    let most = with_command_header(&bytes, |h| put(h, 0, u32::MAX.into(), 4));
    assert_eq!(
        CreditView::from_frame(&frame_of(&most)?)?.frames(),
        u32::MAX
    );
    Ok(())
}

#[test]
fn test_a_grant_of_nothing_is_refused() -> TestResult {
    let zero = with_command_header(&golden("credit")?.bytes()?, |h| put(h, 0, 0, 4));
    assert_eq!(
        CreditView::from_frame(&frame_of(&zero)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::FieldValue {
            command: 14,
            field: "frames",
            value: 0
        }
    );
    Ok(())
}
