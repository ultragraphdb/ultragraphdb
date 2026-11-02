/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyView, Cancel, CancelView};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_bodies::{frame_of, golden};

#[test]
fn test_the_golden_cancel_reads() -> TestResult {
    let bytes = golden("cancel")?.bytes()?;
    assert_eq!(
        CancelView::from_frame(&frame_of(&bytes)?)?.to_body()?,
        Cancel
    );
    assert_eq!(frame_of(&bytes)?.header().stream(), 11);
    Ok(())
}
