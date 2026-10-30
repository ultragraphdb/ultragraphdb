/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_client_core::{ClientError, ClientErrorEnum};
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::TransportError;
use ultragraphdb_protocol_wire::{ErrorCode, RequestTerm};

fn every() -> Vec<ClientError> {
    vec![
        ClientError::Transport(TransportError::Closed()),
        ClientError::Protocol(ProtocolError::FrameTooLarge(9, 8)),
        ClientError::Refused(
            ErrorCode::Conflict,
            3,
            "conflict",
            Some(RequestTerm::Iri("http://ex.org/v".into())),
        ),
        ClientError::Closed(ErrorCode::Protocol, "bye"),
        ClientError::TimedOut(),
        ClientError::ConnectionLost(),
        ClientError::NotConnected(),
        ClientError::Request(ProtocolError::FrameTooLarge(9, 8)),
        ClientError::ReplyMismatch("session", 1, 2),
        ClientError::RetriesExhausted(9, 7, 8),
        ClientError::OutcomeUnknown(9, 7),
        ClientError::InDoubt(9, 7),
        ClientError::NotInDoubt(9),
        ClientError::SessionBusy(9),
        ClientError::UnknownSession(9),
        ClientError::ReadOnly(5),
        ClientError::StreamFinished(5),
        ClientError::StreamBusy(5),
        ClientError::StreamLimit(64),
        ClientError::TransactionMismatch(5, 3),
    ]
}

#[test]
fn test_each_constructor_builds_its_variant() {
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
        ClientError::Refused(ErrorCode::Busy, 0, "m", None).0,
        ClientErrorEnum::Refused {
            code: ErrorCode::Busy,
            detail: 0,
            message: "m".into(),
            value: None
        }
    );
    assert_eq!(
        ClientError::RetriesExhausted(1, 2, 3).0,
        ClientErrorEnum::RetriesExhausted {
            session: 1,
            request: 2,
            attempts: 3
        }
    );
    assert_eq!(
        ClientError::OutcomeUnknown(1, 2).0,
        ClientErrorEnum::OutcomeUnknown {
            session: 1,
            request: 2
        }
    );
    assert_eq!(
        ClientError::TransactionMismatch(5, 3).0,
        ClientErrorEnum::TransactionMismatch {
            stream: 5,
            named: 3
        }
    );
    assert_eq!(
        ClientError::ReplyMismatch("request", 1, 2).0,
        ClientErrorEnum::ReplyMismatch {
            field: "request",
            expected: 1,
            found: 2
        }
    );
    assert_eq!(
        ClientError::InDoubt(9, 7).0,
        ClientErrorEnum::InDoubt {
            session: 9,
            request: 7
        }
    );
}

#[test]
fn test_retryable_exactly_where_the_wire_says() {
    for code in ErrorCode::ALL {
        assert_eq!(
            ClientError::Refused(code, 0, "", None).retryable(),
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
}

#[test]
fn test_stale_and_closing_are_named() {
    assert!(ClientError::Refused(ErrorCode::StaleStatement, 1, "", None).is_stale());
    assert!(!ClientError::Refused(ErrorCode::UnknownStatement, 1, "", None).is_stale());
    // Transport, Protocol, Closed and TimedOut give the connection up; nothing else does.
    let closing: Vec<bool> = every().iter().map(ClientError::closes_connection).collect();
    assert_eq!(
        closing,
        vec![
            true, true, false, true, true, false, false, false, false, false, false, false, false,
            false, false, false, false, false, false, false
        ]
    );
    assert!(
        every()
            .iter()
            .filter(|e| !matches!(e.0, ClientErrorEnum::Refused { .. }))
            .all(|e| !e.is_stale())
    );
}

#[test]
fn test_each_names_its_values() {
    let texts: Vec<String> = every().iter().map(ToString::to_string).collect();
    assert_eq!(
        texts,
        vec![
            "connection failed: connection closed by the peer",
            "the server sent a frame refused: frame of 9 bytes exceeds the 8-byte limit",
            "refused (Conflict, detail 3): conflict",
            "the server closed the connection (Protocol): bye",
            "no answer within the timeout",
            "the request's connection was lost",
            "no connection is ready",
            "the request cannot be sent: frame of 9 bytes exceeds the 8-byte limit",
            "a reply names session 2 where 1 was asked",
            "commit 7 of session 9 unanswered after 8 attempts: in doubt",
            "commit 7 of session 9: the session was evicted, its outcome is unknown",
            "session 9 has commit 7 in doubt",
            "session 9 has no commit in doubt",
            "session 9 already has a commit in flight",
            "no session 9",
            "the transaction on stream 5 is read-only",
            "stream 5 is not open",
            "stream 5 still waits for its answer",
            "the connection holds 64 streams, as many as allowed",
            "a request names the transaction of stream 3, asked of stream 5's",
        ]
    );
    assert!(every()[0].source().is_none());
}
