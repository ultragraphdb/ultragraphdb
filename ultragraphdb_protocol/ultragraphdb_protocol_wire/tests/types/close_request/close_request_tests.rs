/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, CloseRequest};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_roles::{golden_role, role_bytes};

#[test]
fn test_the_golden_close_request_is_built_byte_for_byte() -> TestResult {
    let golden = golden_role("close_request")?;
    let request = CloseRequest::new(7, 30);
    assert_eq!(request.partition(), 7);
    assert_eq!(request.epoch(), 30);
    assert_eq!(request.frame(golden.header())?, role_bytes(&golden)?);
    Ok(())
}

#[test]
fn test_the_fields_sit_at_their_offsets() {
    let header = CloseRequest::new(0x0403_0201, 0x0000_0302_0100_0908).command_header();
    assert_eq!(&header[0..4], &[1, 2, 3, 4]);
    assert_eq!(&header[4..8], &[0; 4]);
    assert_eq!(&header[8..16], &0x0000_0302_0100_0908u64.to_le_bytes());
    assert!(header[16..].iter().all(|b| *b == 0));
    assert!(CloseRequest::new(1, 1).body().is_empty());
}
