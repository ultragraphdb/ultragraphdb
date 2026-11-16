/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{CheckOutcome, ErrorCode, ErrorReply};

use crate::types::recall_answer::RecallAnswer;
use crate::types::recall_decision::RecallDecision;

/// What every partition's `answers` to a Recall call for, the first that applies in `PROTOCOL.md` §11.5's order: a stored
/// reply; code 14; code 17; code 13, `detail` the first such partition's last; and, every partition having answered 18, a
/// second Check of kind 1. No partition asked is code 17: none could be asked.
#[must_use]
pub fn recall_decision(answers: &[RecallAnswer]) -> RecallDecision {
    if answers.contains(&RecallAnswer::Stored) {
        return RecallDecision::Stored;
    }
    if answers.contains(&RecallAnswer::Busy) {
        return RecallDecision::Refused(ErrorReply::new(
            ErrorCode::SessionBusy,
            "the commit is still in a partition's commit step",
        ));
    }
    if answers.is_empty() || answers.contains(&RecallAnswer::Unavailable) {
        return RecallDecision::Refused(ErrorReply::new(
            ErrorCode::Unavailable,
            "a partition could not be asked, and the commit stays in doubt",
        ));
    }
    let out_of_order = answers.iter().find_map(|answer| match answer {
        RecallAnswer::OutOfOrder(last) => Some(*last),
        RecallAnswer::Stored
        | RecallAnswer::Busy
        | RecallAnswer::Unavailable
        | RecallAnswer::NotCommitted => None,
    });
    match out_of_order {
        Some(last) => RecallDecision::Refused(
            ErrorReply::new(
                ErrorCode::RequestOutOfOrder,
                format!("the session's last request here is {last}"),
            )
            .with_detail(last),
        ),
        None => RecallDecision::SecondCheck,
    }
}

/// The answer to a retried Commit of `request` that every partition answered 18, by the second Check's `outcome`: code 12,
/// 21 (`detail` 2) and 14 as it gave them, and code 18, `detail` the request, when it passed.
#[must_use]
pub fn second_check_answer(outcome: CheckOutcome, request: u64) -> ErrorReply {
    match check_refusal(outcome) {
        Ok(()) => ErrorReply::new(
            ErrorCode::NotCommitted,
            format!("request {request} was not committed"),
        )
        .with_detail(request),
        Err(refusal) => refusal,
    }
}

/// What a Check's `outcome` refuses its request with: nothing when it passed; code 12, code 21 with `detail` 2 (`write`),
/// or code 14 (`PROTOCOL.md` §11.5); a reply left unanswered, which no reply the wire's view takes holds, code 17.
///
/// # Errors
///
/// The refusal, for every outcome but passed.
pub fn check_refusal(outcome: CheckOutcome) -> Result<(), ErrorReply> {
    match outcome {
        CheckOutcome::Passed => Ok(()),
        CheckOutcome::NotHeld => Err(ErrorReply::new(
            ErrorCode::SessionEvicted,
            "the registry holds no such session: it was evicted, or never registered",
        )),
        CheckOutcome::OtherPrincipal => Err(ErrorReply::new(
            ErrorCode::PermissionDenied,
            "the session belongs to another principal",
        )
        .with_detail(2)),
        CheckOutcome::InFlight => Err(ErrorReply::new(
            ErrorCode::SessionBusy,
            "the session has a request in flight",
        )),
        CheckOutcome::Unanswered => Err(ErrorReply::new(
            ErrorCode::Unavailable,
            "the system group left the check unanswered",
        )),
    }
}
