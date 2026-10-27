/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, PatternSlot, QuadPattern, Read, TermTable};

use crate::utils::golden_bodies::golden;

const EX: &str = "http://ex.org/";

fn ex(name: &str) -> String {
    format!("{EX}{name}")
}

/// The golden Read: terms <a> and <name>, the pattern (?, <a>, <name>, ?).
pub fn golden_read() -> Read {
    let mut terms = TermTable::new();
    let a = terms.iri(&ex("a"));
    let name = terms.iri(&ex("name"));
    let pattern = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Bound(a),
        PatternSlot::Bound(name),
        PatternSlot::Unbound,
    );
    Read::new(0, 100, terms, pattern, vec![1, 2, 3])
}

#[test]
fn test_read_builds_the_golden_frame() {
    let read = golden_read();
    assert_eq!((read.namespace(), read.limit()), (0, 100));
    assert_eq!(read.resume(), &[1, 2, 3]);
    assert_eq!(read.terms().len(), 2);
    assert_eq!(read.pattern().parameters(), 0);
    assert_eq!(
        read.frame(golden("read").header()).unwrap(),
        golden("read").bytes()
    );
}
