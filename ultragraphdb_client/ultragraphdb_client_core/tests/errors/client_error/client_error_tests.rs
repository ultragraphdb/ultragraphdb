/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_client_core::{ClientError, ClientErrorEnum};
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::TransportError;
use ultragraphdb_protocol_wire::{ErrorCode, RefusedRequest, RequestTerm, RunsAgain};
use ultragraphdb_tool_bdd::TestResult;

fn every() -> Vec<ClientError> {
    vec![
        ClientError::transport(TransportError::closed()),
        ClientError::protocol(ProtocolError::frame_too_large(9, 8)),
        ClientError::refused(
            ErrorCode::Conflict,
            3,
            "conflict",
            Some(RequestTerm::Iri("http://ex.org/v".into())),
        ),
        ClientError::closed(ErrorCode::Protocol, "bye"),
        ClientError::timed_out(),
        ClientError::connection_lost(Some(ClientError::protocol(ProtocolError::frame_too_large(
            9, 8,
        )))),
        ClientError::not_connected(),
        ClientError::request(ProtocolError::frame_too_large(9, 8)),
        ClientError::reply_mismatch("session", 1u64, 2u64),
        ClientError::outcome_unknown(9, 7),
        ClientError::in_doubt(9, 7, Some(ClientError::timed_out())),
        ClientError::not_in_doubt(9),
        ClientError::session_busy(9),
        ClientError::unknown_session(9),
        ClientError::read_only(5),
        ClientError::stream_finished(5),
        ClientError::stream_busy(5),
        ClientError::stream_limit(64),
        ClientError::buffer_limit(8),
        ClientError::transaction_mismatch(5, 3),
    ]
}

#[test]
fn test_each_constructor_builds_its_variant() -> TestResult {
    let kinds: Vec<_> = every()
        .into_iter()
        .map(|e| std::mem::discriminant(&e.0))
        .collect();
    // ClientErrorEnum's 20 variants, each once.
    assert_eq!(kinds.len(), 20);
    assert!(
        kinds
            .iter()
            .enumerate()
            .all(|(i, k)| !kinds[..i].contains(k)),
        "every variant once"
    );
    assert_eq!(
        ClientError::refused(ErrorCode::Busy, 0, "m", None).0,
        ClientErrorEnum::Refused {
            request: RefusedRequest::Other,
            code: ErrorCode::Busy,
            detail: 0,
            message: "m".into(),
            value: None
        }
    );
    assert_eq!(
        ClientError::outcome_unknown(1, 2).0,
        ClientErrorEnum::OutcomeUnknown {
            session: 1,
            request: 2
        }
    );
    assert_eq!(
        ClientError::transaction_mismatch(5, 3).0,
        ClientErrorEnum::TransactionMismatch {
            stream: 5,
            named: 3
        }
    );
    assert_eq!(
        ClientError::reply_mismatch("request", 1u64, 2u64).0,
        ClientErrorEnum::ReplyMismatch {
            field: "request",
            expected: 1,
            found: 2
        }
    );
    assert_eq!(
        ClientError::buffer_limit(8).0,
        ClientErrorEnum::BufferLimit { buffers: 8 }
    );
    assert_eq!(
        ClientError::in_doubt(9, 7, None).0,
        ClientErrorEnum::InDoubt {
            session: 9,
            request: 7,
            cause: None
        }
    );
    Ok(())
}

/// A refusal runs again what its code says for its request, and a commit in doubt runs nothing until it is resolved.
#[test]
fn test_what_runs_again_is_the_codes_answer_for_its_request() {
    let kinds = [
        RefusedRequest::Read,
        RefusedRequest::Commit,
        RefusedRequest::OneFrame,
        RefusedRequest::Other,
    ];
    for code in ErrorCode::ALL {
        for kind in kinds {
            let refused = ClientError::refused(code, 0, "", None).of_request(kind);
            assert_eq!(
                refused.runs_again(),
                code.runs_again(kind),
                "{code:?} {kind:?}"
            );
            assert_eq!(
                refused.retryable(),
                code.runs_again(kind)
                    .is_some_and(|runs| runs != RunsAgain::InDoubt),
                "{code:?} {kind:?}"
            );
        }
    }
    let doubt = ClientError::in_doubt(9, 7, None);
    assert_eq!(doubt.runs_again(), Some(RunsAgain::InDoubt));
    assert!(!doubt.retryable());
    assert_eq!(ClientError::timed_out().runs_again(), None);
    assert_eq!(
        ClientError::timed_out().of_request(RefusedRequest::Read),
        ClientError::timed_out(),
        "only a refusal names its request"
    );
}

#[test]
fn test_retryable_exactly_where_the_wire_says() -> TestResult {
    for code in ErrorCode::ALL {
        // Code 17 is no retry of a request of no kind.
        assert_eq!(
            ClientError::refused(code, 0, "", None).retryable(),
            code.retryable(),
            "{code:?}"
        );
    }
    assert!(
        every()
            .iter()
            .filter(|e| !matches!(e.0, ClientErrorEnum::Refused { .. }))
            .all(|e| !e.retryable())
    );
    Ok(())
}

#[test]
fn test_stale_and_closing_are_named() -> TestResult {
    assert!(ClientError::refused(ErrorCode::StaleStatement, 1, "", None).is_stale());
    assert!(!ClientError::refused(ErrorCode::UnknownStatement, 1, "", None).is_stale());
    // Transport, Protocol, Closed, TimedOut and ConnectionLost give the connection up, or come of its loss; nothing
    // else does.
    let closing: Vec<bool> = every().iter().map(ClientError::closes_connection).collect();
    assert_eq!(
        closing,
        vec![
            true, true, false, true, true, true, false, false, false, false, false, false, false,
            false, false, false, false, false, false, false
        ]
    );
    assert!(
        every()
            .iter()
            .filter(|e| !matches!(e.0, ClientErrorEnum::Refused { .. }))
            .all(|e| !e.is_stale())
    );
    Ok(())
}

#[test]
fn test_each_names_its_values() -> TestResult {
    let texts: Vec<String> = every().iter().map(ToString::to_string).collect();
    assert_eq!(
        texts,
        vec![
            "connection failed: connection closed by the peer",
            "the server sent a frame refused: frame of 9 bytes exceeds the 8-byte limit",
            "refused (Conflict, detail 3): conflict",
            "the server closed the connection (Protocol): bye",
            "no answer within the timeout",
            "the request's connection was lost: the server sent a frame refused: frame of 9 bytes exceeds the 8-byte limit",
            "no connection is ready",
            "the request cannot be sent: frame of 9 bytes exceeds the 8-byte limit",
            "a reply names session 2 where 1 was asked",
            "commit 7 of session 9: its outcome is unknown, and it is not retried",
            "session 9 has commit 7 in doubt: no answer within the timeout",
            "session 9 has no commit in doubt",
            "session 9 already has a commit in flight",
            "no session 9",
            "the transaction on stream 5 is read-only",
            "stream 5 is not open",
            "stream 5 still waits for its answer",
            "the connection holds 64 streams, as many as allowed",
            "the client's 8 frame buffers hold its open reads: one is read to its end or cancelled first",
            "a request names the transaction of stream 3, asked of stream 5's",
        ]
    );
    // Only a lost connection and a commit in doubt with a cause have a source: every other variant has none.
    let sources: Vec<bool> = every().iter().map(|e| e.source().is_some()).collect();
    let caused: Vec<bool> = every()
        .iter()
        .map(|e| {
            matches!(
                e.0,
                ClientErrorEnum::ConnectionLost { .. } | ClientErrorEnum::InDoubt { .. }
            )
        })
        .collect();
    assert_eq!(sources, caused);
    Ok(())
}

/// A lost connection names what gave it up, in its text and as its source; with nothing to name, neither.
#[test]
fn test_a_lost_connection_names_its_cause() -> TestResult {
    let cause = ClientError::transport(TransportError::truncated(100, 256));
    let lost = ClientError::connection_lost(Some(cause.clone()));
    assert_eq!(
        lost.0,
        ClientErrorEnum::ConnectionLost {
            cause: Some(Box::new(cause.clone()))
        }
    );
    assert_eq!(
        lost.source().and_then(|s| s.downcast_ref::<ClientError>()),
        Some(&cause)
    );
    assert_eq!(
        lost.to_string(),
        // ClientError's "connection failed: {error}" around TransportError's Truncated text.
        "the request's connection was lost: connection failed: connection closed 100 bytes into a frame of 256"
    );
    let plain = ClientError::connection_lost(None);
    assert_eq!(plain.0, ClientErrorEnum::ConnectionLost { cause: None });
    assert!(plain.source().is_none());
    assert_eq!(plain.to_string(), "the request's connection was lost");
    assert!(plain.closes_connection());
    Ok(())
}
