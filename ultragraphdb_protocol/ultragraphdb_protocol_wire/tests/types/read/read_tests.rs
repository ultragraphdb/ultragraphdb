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
    Read::new(5, 100, terms, pattern, vec![1, 2, 3]).with_statement_ids()
}

#[test]
fn test_read_builds_the_golden_frame() {
    let read = golden_read();
    assert_eq!((read.transaction(), read.limit()), (5, 100));
    assert!(read.statement_ids());
    assert_eq!(read.resume(), &[1, 2, 3]);
    assert_eq!(read.terms().len(), 2);
    assert_eq!(read.pattern().parameters(), 0);
    assert_eq!(
        read.frame(golden("read").header()).unwrap(),
        golden("read").bytes()
    );
}

#[test]
fn test_a_read_asks_for_no_statement_ids_unless_told() {
    let mut terms = TermTable::new();
    let a = terms.iri(&ex("a"));
    let pattern = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Bound(a),
        PatternSlot::Unbound,
        PatternSlot::Unbound,
    );
    let read = Read::new(9, 0, terms, pattern, Vec::new());
    assert!(!read.statement_ids());
    // The command header by the table: transaction at 0, limit at 8, flags at 12, every other byte zero.
    let header = read.command_header();
    assert_eq!(
        &header[..16],
        &[9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    let asked = read.with_statement_ids().command_header();
    assert_eq!(&asked[12..16], &[1, 0, 0, 0]);
    assert!(asked[16..].iter().all(|b| *b == 0));
}
