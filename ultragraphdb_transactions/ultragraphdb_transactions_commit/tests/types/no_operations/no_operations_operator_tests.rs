/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The operator of a step running no commit-time operations: every operation refused, and nothing staged or kept.

use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transactions_commit::{CommitError, NoOperations, Operator};

use crate::utils::proposal_double::{Reads, TestProposal, Writes, proposal};

#[test]
fn test_no_operations_refuses_every_operation_and_stages_nothing() -> TestResult {
    let mut operator = NoOperations;
    let mut asking: TestProposal = proposal(0, &Reads::default(), &Writes::point(b"k"))?;
    let mut results = Vec::new();
    <NoOperations as Operator<TestProposal>>::begin_round(&mut operator);
    let refused = operator
        .operate(&asking, std::iter::empty(), &mut results)
        .err()
        .context("refused")?;
    assert_eq!(
        refused,
        CommitError::refused("the commit step runs no commit-time operations".to_string())
    );
    assert!(results.is_empty());
    let staged = ();
    assert!(<NoOperations as Operator<TestProposal>>::staged_record(&operator, &staged).is_empty());
    assert_eq!(
        <NoOperations as Operator<TestProposal>>::staged_dictionary(&operator, &staged).count(),
        0
    );
    let before = asking.clone();
    operator.adopt(&mut asking, ());
    operator.discard(&asking, ());
    operator.admitted(&asking);
    assert_eq!(
        asking.record, before.record,
        "the proposal is left as it was"
    );
    assert_eq!(operator, NoOperations, "nothing is kept");
    Ok(())
}
