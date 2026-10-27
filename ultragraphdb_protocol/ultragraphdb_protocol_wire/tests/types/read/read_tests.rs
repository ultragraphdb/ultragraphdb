/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, PatternSlot, QuadPattern, Read};

use crate::utils::golden_bodies::golden;

pub fn golden_read() -> Read {
    Read::new(
        0,
        100,
        QuadPattern::new(
            PatternSlot::Unbound,
            PatternSlot::Bound("<http://ex.org/a>".to_string()),
            PatternSlot::Bound("<http://ex.org/name>".to_string()),
            PatternSlot::Unbound,
        ),
        vec![1, 2, 3],
    )
}

#[test]
fn test_read_builds_the_golden_frame() {
    let read = golden_read();
    assert_eq!(read.namespace(), 0);
    assert_eq!(read.limit(), 100);
    assert_eq!(read.resume(), &[1, 2, 3]);
    assert_eq!(read.pattern().parameters(), 0);
    assert_eq!(
        read.frame(golden("read").header()).unwrap(),
        golden("read").bytes()
    );
}
