/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, Hello, MAX_FRAME_SIZE};

use crate::utils::golden_bodies::{RELEASE, golden};

#[test]
fn test_hello_builds_the_golden_frame() {
    let hello = Hello::new(1, 1, RELEASE, MAX_FRAME_SIZE as u32, "ultragraph-test");
    assert_eq!(
        hello.frame(golden("hello").header()).unwrap(),
        golden("hello").bytes()
    );
}

#[test]
fn test_hello_keeps_its_fields() {
    let hello = Hello::new(2, 7, 0x0102_0304, 0x000A_0B0C, "c");
    assert_eq!(hello.protocol_min(), 2);
    assert_eq!(hello.protocol_max(), 7);
    assert_eq!(hello.release(), 0x0102_0304);
    assert_eq!(hello.max_frame(), 0x000A_0B0C);
    assert_eq!(hello.client(), "c");
    // The command header by the table: min at 0, max at 2, release at 4, max_frame at 8.
    let header = hello.command_header();
    assert_eq!(
        &header[..12],
        &[2, 0, 7, 0, 4, 3, 2, 1, 0x0C, 0x0B, 0x0A, 0]
    );
    assert!(header[12..].iter().all(|b| *b == 0));
}
