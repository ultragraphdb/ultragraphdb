/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_transactions_commit::{CommitError, Reply};

#[test]
fn test_a_reply_keeps_its_ticket_and_outcome() {
    let committed = Reply::new(3, Ok(12));
    assert_eq!(committed.ticket(), 3);
    assert_eq!(committed.outcome(), &Ok(12));
    let refused = Reply::new(4, Err(CommitError::Unavailable()));
    assert_eq!(refused.outcome(), &Err(CommitError::Unavailable()));
}

#[test]
fn test_a_reply_carries_its_operations_results() {
    use ultragraphdb_protocol_wire::OperationResult;
    let plain = Reply::new(3, Ok(12));
    assert!(plain.results().is_empty());
    let results = vec![
        OperationResult::new(9, true),
        OperationResult::new(0, false),
    ];
    let with = Reply::new(3, Ok(12)).with_results(results.clone());
    assert_eq!(with.results(), results.as_slice());
    assert_eq!(with.outcome(), &Ok(12));
    assert_ne!(with, plain);
}
