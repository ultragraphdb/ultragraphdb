/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod client_error_display;
mod client_error_error;

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::TransportError;
use ultragraphdb_protocol_wire::ErrorCode;

/// Why a client operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientErrorEnum {
    /// The connection failed or closed; requests in flight on it are lost, a commit is retried through its session.
    Transport { error: TransportError },
    /// A frame the server sent broke a validation rule; the client closes the connection.
    Protocol { error: ProtocolError },
    /// The server refused the request on its stream, by the command's own rules; the connection stays open.
    Refused {
        code: ErrorCode,
        detail: u64,
        message: String,
        value: Option<String>,
    },
    /// The server closed the connection with an Error frame on stream 0.
    Closed { code: ErrorCode, message: String },
    /// Nothing arrived within the client's timeout; the client closes the connection.
    TimedOut,
    /// The request's connection was lost before it was answered; it is not retried.
    ConnectionLost,
    /// No connection is ready: none was made, or the handshake has not finished.
    NotConnected,
    /// A request could not be built: a term table refused, or a frame past `max_frame` bytes.
    Request { error: ProtocolError },
    /// A reply named another `field` value than the request it answers.
    ReplyMismatch {
        field: &'static str,
        expected: u64,
        found: u64,
    },
    /// The commit of request `request` of session `session` was retried `attempts` times without an answer: whether
    /// it was applied is unknown until the session is resolved.
    RetriesExhausted {
        session: u64,
        request: u64,
        attempts: u32,
    },
    /// Session `session` has the commit of `request` in doubt; it is resolved before it commits again.
    InDoubt { session: u64, request: u64 },
    /// Session `session` has no commit in doubt to resolve.
    NotInDoubt { session: u64 },
    /// Session `session` already has a commit in flight: one at a time per session.
    SessionBusy { session: u64 },
    /// The client holds no session `session`.
    UnknownSession { session: u64 },
    /// The transaction of stream `stream` is read-only and was asked to write or commit.
    ReadOnly { stream: u64 },
    /// Stream `stream` is no longer open: finished, refused, cancelled, or its connection lost.
    StreamFinished { stream: u64 },
    /// Stream `stream` still waits for a reply or a result: its request is finished first.
    StreamBusy { stream: u64 },
    /// The connection holds `limit` streams, as many as Welcome allows.
    StreamLimit { limit: u32 },
}

/// Why a client operation failed; `.0` classifies it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientError(pub ClientErrorEnum);

impl ClientError {
    #[allow(non_snake_case)]
    pub fn Transport(error: TransportError) -> Self {
        Self(ClientErrorEnum::Transport { error })
    }

    #[allow(non_snake_case)]
    pub fn Protocol(error: ProtocolError) -> Self {
        Self(ClientErrorEnum::Protocol { error })
    }

    #[allow(non_snake_case)]
    pub fn Refused(
        code: ErrorCode,
        detail: u64,
        message: impl Into<String>,
        value: Option<String>,
    ) -> Self {
        Self(ClientErrorEnum::Refused {
            code,
            detail,
            message: message.into(),
            value,
        })
    }

    #[allow(non_snake_case)]
    pub fn Closed(code: ErrorCode, message: impl Into<String>) -> Self {
        Self(ClientErrorEnum::Closed {
            code,
            message: message.into(),
        })
    }

    #[allow(non_snake_case)]
    pub fn TimedOut() -> Self {
        Self(ClientErrorEnum::TimedOut)
    }

    #[allow(non_snake_case)]
    pub fn ConnectionLost() -> Self {
        Self(ClientErrorEnum::ConnectionLost)
    }

    #[allow(non_snake_case)]
    pub fn NotConnected() -> Self {
        Self(ClientErrorEnum::NotConnected)
    }

    #[allow(non_snake_case)]
    pub fn Request(error: ProtocolError) -> Self {
        Self(ClientErrorEnum::Request { error })
    }

    #[allow(non_snake_case)]
    pub fn ReplyMismatch(field: &'static str, expected: u64, found: u64) -> Self {
        Self(ClientErrorEnum::ReplyMismatch {
            field,
            expected,
            found,
        })
    }

    #[allow(non_snake_case)]
    pub fn RetriesExhausted(session: u64, request: u64, attempts: u32) -> Self {
        Self(ClientErrorEnum::RetriesExhausted {
            session,
            request,
            attempts,
        })
    }

    #[allow(non_snake_case)]
    pub fn InDoubt(session: u64, request: u64) -> Self {
        Self(ClientErrorEnum::InDoubt { session, request })
    }

    #[allow(non_snake_case)]
    pub fn NotInDoubt(session: u64) -> Self {
        Self(ClientErrorEnum::NotInDoubt { session })
    }

    #[allow(non_snake_case)]
    pub fn SessionBusy(session: u64) -> Self {
        Self(ClientErrorEnum::SessionBusy { session })
    }

    #[allow(non_snake_case)]
    pub fn UnknownSession(session: u64) -> Self {
        Self(ClientErrorEnum::UnknownSession { session })
    }

    #[allow(non_snake_case)]
    pub fn ReadOnly(stream: u64) -> Self {
        Self(ClientErrorEnum::ReadOnly { stream })
    }

    #[allow(non_snake_case)]
    pub fn StreamFinished(stream: u64) -> Self {
        Self(ClientErrorEnum::StreamFinished { stream })
    }

    #[allow(non_snake_case)]
    pub fn StreamBusy(stream: u64) -> Self {
        Self(ClientErrorEnum::StreamBusy { stream })
    }

    #[allow(non_snake_case)]
    pub fn StreamLimit(limit: u32) -> Self {
        Self(ClientErrorEnum::StreamLimit { limit })
    }

    /// Whether the application retries the operation, exactly where the wire says so: a refusal whose code is
    /// retryable ([`ErrorCode::retryable`]: conflict, too old, past the limit, busy).
    pub fn retryable(&self) -> bool {
        matches!(&self.0, ClientErrorEnum::Refused { code, .. } if code.retryable())
    }

    /// Whether a prepared statement was refused as stale: its namespace's catalog changed, and preparing it again
    /// succeeds.
    pub fn is_stale(&self) -> bool {
        matches!(
            &self.0,
            ClientErrorEnum::Refused {
                code: ErrorCode::StaleStatement,
                ..
            }
        )
    }

    /// Whether the connection is gone with this error, so the next request reconnects.
    pub fn closes_connection(&self) -> bool {
        matches!(
            &self.0,
            ClientErrorEnum::Transport { .. }
                | ClientErrorEnum::Protocol { .. }
                | ClientErrorEnum::Closed { .. }
                | ClientErrorEnum::TimedOut
        )
    }
}
