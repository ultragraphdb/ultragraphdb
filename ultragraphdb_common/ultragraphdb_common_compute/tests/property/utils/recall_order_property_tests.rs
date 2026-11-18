/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A check's outcome and a Recall's answers: laws L5.7 and L5.8 of `notes/laws.md` (`PROTOCOL.md` §11.5).

use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_common_compute::{
    RecallAnswer, RecallDecision, check_refusal, recall_decision, second_check_answer,
};
use ultragraphdb_protocol_wire::CheckOutcome;

const OUTCOMES: [CheckOutcome; 4] = [
    CheckOutcome::Passed,
    CheckOutcome::NotHeld,
    CheckOutcome::InFlight,
    CheckOutcome::OtherPrincipal,
];

/// The code each outcome refuses with, from `PROTOCOL.md` §11.5: none for passed, 12, 14 and 21.
fn code_of(outcome: CheckOutcome) -> Option<u16> {
    match outcome {
        CheckOutcome::Passed => None,
        CheckOutcome::NotHeld => Some(12),
        CheckOutcome::InFlight => Some(14),
        CheckOutcome::OtherPrincipal => Some(21),
        CheckOutcome::Unanswered => Some(17),
    }
}

/// L5.7: a request is routed exactly when its check's outcome is 1, and refused otherwise with the outcome's code.
#[hegel::test(test_cases = 40)]
fn test_a_request_is_routed_exactly_when_its_check_passed(tc: TestCase) {
    let outcome = tc.draw(gs::sampled_from(OUTCOMES.to_vec()));
    let refused = check_refusal(outcome).err();
    assert_eq!(
        refused.as_ref().map(|r| r.code().number()),
        code_of(outcome)
    );
    if outcome == CheckOutcome::OtherPrincipal {
        assert_eq!(refused.map(|r| r.detail()), Some(2), "`write`");
    }
}

/// L5.8: the answer to a retried Commit is the first that applies of a stored reply, 14, 17, 13 and the second Check's
/// verdict, whatever the order the partitions answered in; the second Check is asked only when every partition answered 18.
#[hegel::test(test_cases = 80)]
fn test_a_recalls_answer_follows_its_order(tc: TestCase) {
    let count = tc.draw(gs::integers::<usize>().min_value(1).max_value(4));
    let answers: Vec<RecallAnswer> = (0..count)
        .map(
            |_| match tc.draw(gs::integers::<u8>().min_value(0).max_value(4)) {
                0 => RecallAnswer::Stored,
                1 => RecallAnswer::Busy,
                2 => RecallAnswer::Unavailable,
                3 => RecallAnswer::OutOfOrder(
                    tc.draw(gs::integers::<u64>().min_value(1).max_value(99)),
                ),
                _ => RecallAnswer::NotCommitted,
            },
        )
        .collect();
    let decision = recall_decision(&answers);
    let first_out_of_order = answers.iter().find_map(|answer| match answer {
        RecallAnswer::OutOfOrder(last) => Some(*last),
        _ => None,
    });
    if answers.contains(&RecallAnswer::Stored) {
        assert_eq!(decision, RecallDecision::Stored);
    } else if answers.contains(&RecallAnswer::Busy) {
        assert!(matches!(&decision, RecallDecision::Refused(r) if r.code().number() == 14));
    } else if answers.contains(&RecallAnswer::Unavailable) {
        assert!(matches!(&decision, RecallDecision::Refused(r) if r.code().number() == 17));
    } else if let Some(last) = first_out_of_order {
        assert!(
            matches!(&decision, RecallDecision::Refused(r) if r.code().number() == 13 && r.detail() == last)
        );
    } else {
        assert_eq!(decision, RecallDecision::SecondCheck);
        let outcome = tc.draw(gs::sampled_from(OUTCOMES.to_vec()));
        let request = tc.draw(gs::integers::<u64>().min_value(1).max_value(99));
        let answer = second_check_answer(outcome, request);
        let expected = match outcome {
            CheckOutcome::Passed => 18,
            other => code_of(other).unwrap_or(0),
        };
        assert_eq!(answer.code().number(), expected);
    }
}
