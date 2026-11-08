/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The test server's reads: one whose transaction's token names no epoch past the closed one is answered at once.

use ultragraphdb_client_core::{BatchPoll, Transaction};
use ultragraphdb_core_ids::{Epoch, SessionToken};
use ultragraphdb_protocol_wire::{Begin, PatternSlot, QuadPattern, Read, TermTable};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::harness::{Harness, begun, server};

#[test]
fn test_a_read_at_the_closed_epoch_is_answered_at_once() -> TestResult {
    let closed = 29;
    let mut h = Harness::ready(server().with_closed_epoch(closed).with_results(1))?;
    let begin = Begin::new(0, 0, true)
        .with_deadline_ms(50)
        .with_token(SessionToken::new(1, 5, Epoch::new(closed)?));
    let p = h.core.begin_with(begin, None, h.now)?;
    let tx = begun(h.settle(p)?)?;
    let at = h.now;
    let stream = h.core.read(&tx, &read(&tx), h.now)?;
    let mut epochs = Vec::new();
    let mut failed = None;
    h.until(|h| {
        loop {
            match h.core.next_batch(&stream, h.now, |batch| batch.epoch()) {
                Ok(BatchPoll::Batch(epoch)) => epochs.push(epoch),
                Ok(BatchPoll::Done) => return true,
                Ok(BatchPoll::Wait) => return false,
                Err(error) => {
                    failed = Some(error);
                    return true;
                }
            }
        }
    })?;
    assert_eq!(failed, None);
    assert_eq!(epochs, vec![closed], "read at the token's epoch");
    assert_eq!(h.now, at, "without waiting");
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
