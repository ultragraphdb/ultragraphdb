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

/// A ConditionalWrite breaking a rule of its body, as a declaration in it does, is refused on its stream with code 1.
#[test]
fn test_a_conditional_write_breaking_a_rule_is_refused_with_code_1() -> TestResult {
    use ultragraphdb_client_core::ClientErrorEnum;
    use ultragraphdb_protocol_wire::{
        CommitOperation, ConditionalWrite, Declared, DeclaredPredicate, ErrorCode, WriteChange,
    };
    let mut h = Harness::ready(server())?;
    let mut terms = TermTable::new();
    let predicate = terms.iri("http://ex.org/id");
    let value = terms.iri("http://ex.org/x");
    let write = ConditionalWrite::new(
        0,
        terms,
        vec![WriteChange::Declare(Declared::Predicate(
            DeclaredPredicate::new("http://ex.org/p"),
        ))],
        vec![CommitOperation::InsertIfAbsent {
            graph: None,
            predicate,
            value,
            properties: Vec::new(),
        }],
    );
    let p = h.core.conditional_write(write, None, h.now)?;
    let refused = h.settle(p)?.err().map(|e| e.0);
    assert!(
        matches!(
            refused,
            Some(ClientErrorEnum::Refused {
                code: ErrorCode::Protocol,
                ..
            })
        ),
        "{refused:?}"
    );
    assert_eq!(h.server.applied(), 0);
    Ok(())
}

/// A Prepare of a text is refused: the server prepares quad patterns alone.
#[test]
fn test_a_prepare_of_a_text_is_refused() -> TestResult {
    use ultragraphdb_client_core::ClientErrorEnum;
    use ultragraphdb_protocol_wire::{ErrorCode, Language, Prepare};
    let mut h = Harness::ready(server())?;
    let prepare = Prepare::of_text(
        0,
        Language::Sparql,
        "SELECT * WHERE { ?s ?p ?o }",
        TermTable::new(),
        Vec::new(),
        Vec::new(),
    );
    let p = h.core.prepare(&prepare, h.now)?;
    let refused = h.settle(p)?.err().map(|e| e.0);
    assert!(
        matches!(
            refused,
            Some(ClientErrorEnum::Refused {
                code: ErrorCode::Refused,
                ..
            })
        ),
        "{refused:?}"
    );
    Ok(())
}

/// A Begin breaking a rule of its body, a read version on a read-write transaction, opens no transaction and is not
/// answered: the client's wait for it times out.
#[test]
fn test_a_begin_breaking_a_rule_is_not_answered() -> TestResult {
    use ultragraphdb_client_core::ClientError;
    let mut h = Harness::ready(server())?;
    let p = h.core.begin_with(Begin::new(0, 5, false), None, h.now)?;
    assert_eq!(h.settle(p)?.err(), Some(ClientError::timed_out()));
    assert_eq!(h.server.open_transactions(), 0);
    Ok(())
}
