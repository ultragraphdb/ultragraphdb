/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::CheckOutcome;
use ultragraphdb_tool_bdd::TestResult;

/// `PROTOCOL.md` §13.24's `outcome`: 0 in a request, 1 passed, and the codes 12, 14 and 21.
const NUMBERS: [(CheckOutcome, u8); 5] = [
    (CheckOutcome::Unanswered, 0),
    (CheckOutcome::Passed, 1),
    (CheckOutcome::NotHeld, 12),
    (CheckOutcome::InFlight, 14),
    (CheckOutcome::OtherPrincipal, 21),
];

#[test]
fn test_each_outcome_keeps_its_number() -> TestResult {
    for (outcome, number) in NUMBERS {
        assert_eq!(outcome.number(), number, "{outcome:?}");
        assert_eq!(CheckOutcome::from_number(number), Some(outcome));
    }
    Ok(())
}

#[test]
fn test_all_lists_every_outcome_in_number_order() -> TestResult {
    let all: Vec<CheckOutcome> = NUMBERS.iter().map(|(outcome, _)| *outcome).collect();
    assert_eq!(CheckOutcome::ALL.to_vec(), all);
    Ok(())
}

/// C1.18 at the type: no other number is an outcome, 7 among them.
#[test]
fn test_no_other_number_is_an_outcome() -> TestResult {
    let known: Vec<u8> = (0..=u8::MAX)
        .filter(|number| CheckOutcome::from_number(*number).is_some())
        .collect();
    assert_eq!(known, [0, 1, 12, 14, 21]);
    Ok(())
}
