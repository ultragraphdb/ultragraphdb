/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_client_core::{ClientError, ClientErrorEnum};
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::TransportError;
use ultragraphdb_protocol_wire::ErrorCode;

fn every() -> Vec<ClientError> {
    vec![
        ClientError::Transport(TransportError::Closed()),
        ClientError::Protocol(ProtocolError::FrameTooLarge(9, 8)),
        ClientError::Refused(ErrorCode::Conflict, 3, "conflict", Some("v".into())),
        ClientError::Closed(ErrorCode::Protocol, "bye"),
        ClientError::TimedOut(),
        ClientError::ConnectionLost(),
        ClientError::NotConnected(),
        ClientError::Request(ProtocolError::FrameTooLarge(9, 8)),
        ClientError::ReplyMismatch("session", 1, 2),
        ClientError::RetriesExhausted(9, 7, 8),
        ClientError::InDoubt(9, 7),
        ClientError::NotInDoubt(9),
        ClientError::SessionBusy(9),
        ClientError::UnknownSession(9),
        ClientError::ReadOnly(5),
        ClientError::StreamFinished(5),
        ClientError::StreamBusy(5),
        ClientError::StreamLimit(64),
    ]
}

#[test]
fn test_each_constructor_builds_its_variant() {
    let kinds: Vec<_> = every()
        .into_iter()
        .map(|e| std::mem::discriminant(&e.0))
        .collect();
    assert_eq!(kinds.len(), 18);
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
    let closing: Vec<bool> = every().iter().map(ClientError::closes_connection).collect();
    assert_eq!(
        closing,
        vec![
            true, true, false, true, true, false, false, false, false, false, false, false, false,
            false, false, false, false, false
        ]
    );
}

#[test]
fn test_each_names_its_values() {
    let texts: Vec<String> = every().iter().map(ToString::to_string).collect();
    assert!(texts[2].contains("conflict") && texts[2].contains("detail 3"));
    assert!(texts[9].contains("commit 7 of session 9") && texts[9].contains("8 attempts"));
    assert!(texts[8].contains("session 2") && texts[8].contains("1"));
    assert!(texts.iter().all(|t| !t.is_empty()));
    assert!(every()[0].source().is_none());
}
