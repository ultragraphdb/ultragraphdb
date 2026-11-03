/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, PatternSlot, QuadPattern, Read, TermTable};
use ultragraphdb_tool_bdd::TestResult;

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
    Read::new(5, 100, terms, pattern, vec![1, 2, 3])
}

#[test]
fn test_read_builds_the_golden_frame() -> TestResult {
    let read = golden_read();
    assert_eq!((read.transaction(), read.limit()), (5, 100));
    assert_eq!(read.resume(), &[1, 2, 3]);
    assert_eq!(read.terms().len(), 2);
    assert_eq!(read.pattern().parameters(), 0);
    assert_eq!(
        read.frame(golden("read")?.header())?,
        golden("read")?.bytes()?
    );
    Ok(())
}

#[test]
fn test_a_read_names_no_level_and_no_flags() {
    let mut terms = TermTable::new();
    let a = terms.iri(&ex("a"));
    let pattern = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Bound(a),
        PatternSlot::Unbound,
        PatternSlot::Unbound,
    );
    let read = Read::new(9, 7, terms, pattern, Vec::new());
    // The command header by the table: transaction at 0, limit at 8, every other byte zero.
    let header = read.command_header();
    assert_eq!(&header[..12], &[9, 0, 0, 0, 0, 0, 0, 0, 7, 0, 0, 0]);
    assert!(header[12..].iter().all(|b| *b == 0));
}
