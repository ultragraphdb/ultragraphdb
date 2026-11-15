/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{CheckKind, CheckOutcome, SessionCheck};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::system_frames::{SESSION_A, SESSION_B};

#[test]
fn test_a_requests_check_holds_no_outcome() -> TestResult {
    let check = SessionCheck::new(SESSION_A, 9, CheckKind::Marks);
    assert_eq!(
        (check.session(), check.principal(), check.kind()),
        (SESSION_A, 9, CheckKind::Marks)
    );
    assert_eq!(check.outcome(), CheckOutcome::Unanswered);
    Ok(())
}

#[test]
fn test_a_replys_check_holds_its_outcome_and_the_fields_sent() -> TestResult {
    let sent = SessionCheck::new(SESSION_B, 11, CheckKind::Asks);
    for outcome in CheckOutcome::ALL {
        let answered = sent.with_outcome(outcome);
        assert_eq!(answered.outcome(), outcome);
        assert_eq!(
            (answered.session(), answered.principal(), answered.kind()),
            (SESSION_B, 11, CheckKind::Asks)
        );
    }
    assert_ne!(sent.with_outcome(CheckOutcome::Passed), sent);
    Ok(())
}
