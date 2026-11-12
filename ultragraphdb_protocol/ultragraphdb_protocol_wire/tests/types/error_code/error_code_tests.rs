/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{ErrorCode, RefusedRequest, RunsAgain};

#[test]
fn test_codes_keep_their_numbers() {
    // The table of ErrorCode's docs.
    let numbers: Vec<u16> = ErrorCode::ALL.iter().map(|c| c.number()).collect();
    assert_eq!(numbers, (1..=31).collect::<Vec<u16>>());
    assert_eq!(ErrorCode::Protocol.number(), 1);
    assert_eq!(ErrorCode::Conflict.number(), 2);
    assert_eq!(ErrorCode::OperationFailed.number(), 8);
    assert_eq!(ErrorCode::StaleStatement.number(), 9);
    assert_eq!(ErrorCode::SessionBusy.number(), 14);
    assert_eq!(ErrorCode::Unavailable.number(), 17);
    assert_eq!(ErrorCode::NotCommitted.number(), 18);
    assert_eq!(ErrorCode::AuthenticationFailed.number(), 19);
    assert_eq!(ErrorCode::Unauthenticated.number(), 20);
    assert_eq!(ErrorCode::PermissionDenied.number(), 21);
    assert_eq!(ErrorCode::CredentialExpired.number(), 22);
    // PROTOCOL.md §9.4: CrossPartition is 23, not retryable.
    assert_eq!(ErrorCode::CrossPartition.number(), 23);
    assert!(!ErrorCode::CrossPartition.retryable());
    // PROTOCOL.md §9.4: the read path's codes 24 to 27.
    assert_eq!(ErrorCode::TimedOut.number(), 24);
    assert_eq!(ErrorCode::LevelRefused.number(), 25);
    assert_eq!(ErrorCode::WrongPartition.number(), 26);
    assert_eq!(ErrorCode::AheadOfWatermark.number(), 27);
    // PROTOCOL.md §9.4: stage 6.6's codes 30, EntityDeleted, and 31, Fenced.
    assert_eq!(ErrorCode::EntityDeleted.number(), 30);
    assert_eq!(ErrorCode::Fenced.number(), 31);
    for code in ErrorCode::ALL {
        assert_eq!(ErrorCode::from_number(code.number()), Some(code));
    }
    assert_eq!(ErrorCode::from_number(0), None);
    assert_eq!(ErrorCode::from_number(30), Some(ErrorCode::EntityDeleted));
    assert_eq!(ErrorCode::from_number(31), Some(ErrorCode::Fenced));
    assert_eq!(ErrorCode::from_number(32), None);
    assert_eq!(ErrorCode::from_number(u16::MAX), None);
}

#[test]
fn test_the_retryable_codes_are_the_tables() {
    let retryable: Vec<ErrorCode> = ErrorCode::ALL
        .into_iter()
        .filter(|c| c.retryable())
        .collect();
    assert_eq!(
        retryable,
        vec![
            ErrorCode::Conflict,
            ErrorCode::TooOld,
            ErrorCode::PastLimit,
            ErrorCode::Busy,
            ErrorCode::SessionBusy,
            ErrorCode::NotCommitted,
            ErrorCode::TimedOut,
            ErrorCode::WrongPartition,
            // PROTOCOL.md §9.4: code 31 is retryable after a delay; code 30 is not.
            ErrorCode::Fenced
        ]
    );
}

#[test]
fn test_a_refused_install_or_query_is_not_retried() {
    assert_eq!(ErrorCode::SchemaRefused.number(), 28);
    assert_eq!(ErrorCode::QueryRefused.number(), 29);
    assert!(!ErrorCode::SchemaRefused.retryable());
    assert!(!ErrorCode::QueryRefused.retryable());
}

/// PROTOCOL.md §9.4's paragraph under the table: codes 2, 3, 4, 18 and 26 run the transaction again; 24 the read; 14 the
/// request; 5 the request on a Drop or a one-frame write and the transaction on a Commit; 17 a read's, and a commit's is in
/// doubt; every code the table calls not retryable runs nothing again.
#[test]
fn test_what_runs_again_is_the_tables() {
    let kinds = [
        RefusedRequest::Read,
        RefusedRequest::Commit,
        RefusedRequest::OneFrame,
        RefusedRequest::Other,
    ];
    let transaction = [
        ErrorCode::Conflict,
        ErrorCode::TooOld,
        ErrorCode::PastLimit,
        ErrorCode::NotCommitted,
        ErrorCode::WrongPartition,
    ];
    for code in ErrorCode::ALL {
        for kind in kinds {
            let runs = code.runs_again(kind);
            if transaction.contains(&code) {
                assert_eq!(runs, Some(RunsAgain::Transaction), "{code:?} {kind:?}");
            } else if ![
                ErrorCode::Busy,
                ErrorCode::Fenced,
                ErrorCode::SessionBusy,
                ErrorCode::TimedOut,
                ErrorCode::Unavailable,
            ]
            .contains(&code)
            {
                assert_eq!(runs, None, "{code:?} {kind:?}");
                assert!(!code.retryable(), "{code:?}");
            }
        }
    }
    let commit = RefusedRequest::Commit;
    let one_frame = RefusedRequest::OneFrame;
    let read = RefusedRequest::Read;
    assert_eq!(
        ErrorCode::SessionBusy.runs_again(commit),
        Some(RunsAgain::Request)
    );
    assert_eq!(
        ErrorCode::SessionBusy.runs_again(one_frame),
        Some(RunsAgain::Request)
    );
    assert_eq!(
        ErrorCode::Busy.runs_again(commit),
        Some(RunsAgain::Transaction)
    );
    assert_eq!(
        ErrorCode::Busy.runs_again(one_frame),
        Some(RunsAgain::Request)
    );
    assert_eq!(ErrorCode::TimedOut.runs_again(read), Some(RunsAgain::Read));
    assert_eq!(
        ErrorCode::Unavailable.runs_again(read),
        Some(RunsAgain::Read)
    );
    assert_eq!(
        ErrorCode::Unavailable.runs_again(commit),
        Some(RunsAgain::InDoubt)
    );
    assert_eq!(
        ErrorCode::Unavailable.runs_again(one_frame),
        Some(RunsAgain::InDoubt)
    );
    assert_eq!(
        ErrorCode::Unavailable.runs_again(RefusedRequest::Other),
        None
    );
}

/// PROTOCOL.md §9.4: code 31 runs again as code 5 does, after a delay: a one-frame write the request, every other the
/// transaction; code 30 runs nothing again, a transaction resolving the IRI anew only when the client runs it.
#[test]
fn test_fenced_runs_again_as_busy_and_entity_deleted_not_at_all() {
    for kind in [
        RefusedRequest::Read,
        RefusedRequest::Commit,
        RefusedRequest::OneFrame,
        RefusedRequest::Other,
    ] {
        assert_eq!(
            ErrorCode::Fenced.runs_again(kind),
            ErrorCode::Busy.runs_again(kind),
            "{kind:?}"
        );
        assert_eq!(ErrorCode::EntityDeleted.runs_again(kind), None, "{kind:?}");
    }
    assert!(ErrorCode::Fenced.retryable());
    assert!(!ErrorCode::EntityDeleted.retryable());
}
