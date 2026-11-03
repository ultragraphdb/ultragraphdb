/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Epoch, SessionToken};
use ultragraphdb_protocol_wire::{OperationResult, TermTable};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transactions_commit::{CommitError, Reply};

#[test]
fn test_a_reply_keeps_its_ticket_and_outcome() -> TestResult {
    let committed = Reply::new(3, Ok(12));
    assert_eq!(committed.ticket(), 3);
    assert_eq!(committed.outcome(), &Ok(12));
    assert_eq!(committed.token(), None);
    let refused = Reply::new(4, Err(CommitError::unavailable()));
    assert_eq!(refused.outcome(), &Err(CommitError::unavailable()));
    Ok(())
}

#[test]
fn test_a_reply_carries_its_operations_results_and_their_terms() -> TestResult {
    let plain = Reply::new(3, Ok(12));
    assert!(plain.results().is_empty());
    assert!(plain.terms().is_empty());
    let mut terms = TermTable::new();
    let node = terms.iri("http://ex.org/x");
    let results = vec![
        OperationResult::for_node(node, true),
        OperationResult::counter(7),
    ];
    let with = Reply::new(3, Ok(12)).with_results(results.clone(), terms.clone());
    assert_eq!(with.results(), results.as_slice());
    assert_eq!(with.terms(), &terms);
    assert_eq!(with.outcome(), &Ok(12));
    assert_ne!(with, plain);
    Ok(())
}

#[test]
fn test_a_commits_reply_carries_its_session_token() -> TestResult {
    let token = SessionToken::new(1, 40, Epoch::new(12).context("the epoch")?);
    let reply = Reply::new(5, Ok(50_331_648)).with_token(token.clone());
    assert_eq!(reply.token(), Some(&token));
    assert_eq!(reply.ticket(), 5);
    assert_ne!(reply, Reply::new(5, Ok(50_331_648)));
    Ok(())
}
