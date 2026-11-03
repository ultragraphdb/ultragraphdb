/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, DefaultEntry};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_roles::{golden_role, role_bytes};

#[test]
fn test_the_golden_request_and_reply_are_built_byte_for_byte() -> TestResult {
    let request = DefaultEntry::request(3, "http://ex.org/age");
    assert!(!request.found());
    assert_eq!(request.namespace(), 3);
    assert_eq!(request.predicate(), "http://ex.org/age");
    let golden = golden_role("default_entry")?;
    assert_eq!(request.frame(golden.header())?, role_bytes(&golden)?);
    let reply = request.with_found(true);
    assert!(reply.found());
    let golden_reply = golden_role("default_entry_reply")?;
    assert_eq!(
        reply.frame(golden_reply.header())?,
        role_bytes(&golden_reply)?
    );
    Ok(())
}

#[test]
fn test_the_fields_sit_at_their_offsets() {
    let header = DefaultEntry::request(0x0807_0605_0403_0201, "p")
        .with_found(true)
        .command_header();
    assert_eq!(&header[0..8], &[1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(header[8], 1);
    assert!(header[9..].iter().all(|b| *b == 0));
}
