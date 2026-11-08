/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A read's handle is the read: its clones are the same handle, and another stream, or the same number on another
//! connection, is another read.

use ultragraphdb_client_core::Transaction;
use ultragraphdb_protocol_wire::{PatternSlot, QuadPattern, Read, TermTable};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::harness::{Harness, begun, server};

#[test]
fn test_a_handle_equals_its_clones_and_no_other_reads() -> TestResult {
    let mut h = Harness::ready(server())?;
    let p = h.core.begin(0, true, h.now)?;
    let tx = begun(h.settle(p)?)?;
    let first = h.core.read(&tx, &read(&tx), h.now)?;
    let second = h.core.read(&tx, &read(&tx), h.now)?;
    assert_eq!(first.clone(), first, "a clone is the same handle");
    assert_ne!(first, second, "another stream is another read");
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    let beginning = h.core.begin(0, true, h.now)?;
    let again = begun(h.settle(beginning)?)?;
    let renewed = h.core.read(&again, &read(&again), h.now)?;
    assert_eq!(renewed.stream(), first.stream(), "the same number again");
    assert_ne!(
        renewed, first,
        "the same number on the next connection is another read"
    );
    Ok(())
}

/// A read of `tx`, naming it.
fn read(tx: &Transaction) -> Read {
    let mut terms = TermTable::new();
    let a = terms.iri("http://ex.org/a");
    Read::new(
        tx.stream(),
        0,
        terms,
        QuadPattern::new(
            PatternSlot::Unbound,
            PatternSlot::Bound(a),
            PatternSlot::Unbound,
            PatternSlot::Unbound,
        ),
        Vec::new(),
    )
}
