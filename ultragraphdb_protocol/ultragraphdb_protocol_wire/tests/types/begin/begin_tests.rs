/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{Begin, BodyBuilder};

use crate::utils::golden_bodies::golden;

#[test]
fn test_begin_builds_the_golden_frame() {
    let begin = Begin::new(3, 0xAB_CDEF_0002, true);
    assert_eq!(
        (begin.namespace(), begin.read_version(), begin.read_only()),
        (3, 0xAB_CDEF_0002, true)
    );
    assert_eq!(
        begin.frame(golden("begin").header()).unwrap(),
        golden("begin").bytes()
    );
    // Read-write is mode 0.
    assert_eq!(Begin::new(3, 0, false).command_header()[16], 0);
    assert_eq!(begin.command_header()[16], 1);
}
