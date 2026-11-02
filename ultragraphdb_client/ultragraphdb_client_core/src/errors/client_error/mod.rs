/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod client_error_display;
mod client_error_error;

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::TransportError;
use ultragraphdb_protocol_wire::{ErrorCode, RequestTerm};

/// Why a client operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientErrorEnum {
    /// The connection failed or closed; requests in flight on it are lost, a commit is retried through its session.
    Transport {
        /// The transport's failure.
        error: TransportError,
    },
    /// A frame the server sent broke a validation rule; the client closes the connection.
    Protocol {
        /// The rule the frame broke.
        error: ProtocolError,
    },
    /// The server refused the request on its stream, by the command's own rules; the connection stays open.
    Refused {
        /// The refusal's error code.
        code: ErrorCode,
        /// The code's detail word, as the Error frame carries it.
        detail: u64,
        /// The server's message.
        message: String,
        /// The request's term the refusal names, if it names one.
        value: Option<RequestTerm>,
    },
    /// The server closed the connection with an Error frame on stream 0.
    Closed {
        /// The closing Error's code.
        code: ErrorCode,
        /// The server's message.
        message: String,
    },
    /// Nothing arrived within the client's timeout; the client closes the connection.
    TimedOut,
    /// The request's connection was lost before it was answered; it is not retried. `cause` is what gave the connection
    /// up: the framer's refusal of the bytes ([`ClientErrorEnum::Transport`]), the rule a frame broke
    /// ([`ClientErrorEnum::Protocol`]), the server's closing Error ([`ClientErrorEnum::Closed`]); `None` for a
    /// connection closed or lost with nothing more to say.
    ConnectionLost {
        /// What gave the connection up, if anything did.
        cause: Option<Box<ClientError>>,
    },
    /// No connection is ready: none was made, or the handshake has not finished.
    NotConnected,
    /// A request could not be built: a term table refused, or a frame past `max_frame` bytes.
    Request {
        /// Why the request could not be built.
        error: ProtocolError,
    },
    /// A reply named another `field` value than the request it answers.
    ReplyMismatch {
        /// The reply's field that differs.
        field: &'static str,
        /// The request's value.
        expected: u64,
        /// The reply's value.
        found: u64,
    },
    /// The commit of request `request` of session `session` was retried `attempts` times without an answer: whether
    /// it was applied is unknown until the session is resolved.
    RetriesExhausted {
        /// The session the commit was sent through.
        session: u64,
        /// The commit's request number in the session.
        request: u64,
        /// How many times the commit was sent.
        attempts: u32,
    },
    /// The retry of request `request` of session `session` was refused as evicted (code 12): whether the commit was
    /// applied is unknown, and it is not retried. The session is gone from the client as from the server.
    OutcomeUnknown {
        /// The evicted session.
        session: u64,
        /// The request number of the retried commit.
        request: u64,
    },
    /// Session `session` has the commit of `request` in doubt; it is resolved before it commits again.
    InDoubt {
        /// The session with the commit in doubt.
        session: u64,
        /// The request number of the commit in doubt.
        request: u64,
    },
    /// Session `session` has no commit in doubt to resolve.
    NotInDoubt {
        /// The session asked to resolve.
        session: u64,
    },
    /// Session `session` already has a commit in flight: one at a time per session.
    SessionBusy {
        /// The busy session.
        session: u64,
    },
    /// The client holds no session `session`.
    UnknownSession {
        /// The session asked for.
        session: u64,
    },
    /// The transaction of stream `stream` is read-only and was asked to write or commit.
    ReadOnly {
        /// The read-only transaction's stream.
        stream: u64,
    },
    /// Stream `stream` is no longer open: finished, refused, cancelled, or its connection lost.
    StreamFinished {
        /// The stream no longer open.
        stream: u64,
    },
    /// Stream `stream` still waits for a reply or a result: its request is finished first.
    StreamBusy {
        /// The stream still waiting.
        stream: u64,
    },
    /// The connection holds `limit` streams, as many as Welcome allows.
    StreamLimit {
        /// The streams Welcome allows.
        limit: u32,
    },
    /// A read or execute names the transaction of stream `named`, and was asked of the transaction of `stream`.
    TransactionMismatch {
        /// The stream of the transaction the request was asked of.
        stream: u64,
        /// The stream of the transaction the request names.
        named: u64,
    },
}

/// Why a client operation failed; `.0` classifies it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientError(pub ClientErrorEnum);

impl ClientError {
    /// A [`ClientErrorEnum::Transport`] failure of the connection.
    pub fn transport(error: TransportError) -> Self {
        Self(ClientErrorEnum::Transport { error })
    }

    /// A [`ClientErrorEnum::Protocol`] violation by a frame the server sent.
    pub fn protocol(error: ProtocolError) -> Self {
        Self(ClientErrorEnum::Protocol { error })
    }

    /// A [`ClientErrorEnum::Refused`] request.
    pub fn refused(
        code: ErrorCode,
        detail: u64,
        message: impl Into<String>,
        value: Option<RequestTerm>,
    ) -> Self {
        Self(ClientErrorEnum::Refused {
            code,
            detail,
            message: message.into(),
            value,
        })
    }

    /// A [`ClientErrorEnum::Closed`] connection, closed by the server's Error frame.
    pub fn closed(code: ErrorCode, message: impl Into<String>) -> Self {
        Self(ClientErrorEnum::Closed {
            code,
            message: message.into(),
        })
    }

    /// A [`ClientErrorEnum::TimedOut`] connection.
    pub fn timed_out() -> Self {
        Self(ClientErrorEnum::TimedOut)
    }

    /// A [`ClientErrorEnum::ConnectionLost`] request, with what gave its connection up.
    pub fn connection_lost(cause: Option<ClientError>) -> Self {
        Self(ClientErrorEnum::ConnectionLost {
            cause: cause.map(Box::new),
        })
    }

    /// A request refused as [`ClientErrorEnum::NotConnected`].
    pub fn not_connected() -> Self {
        Self(ClientErrorEnum::NotConnected)
    }

    /// A [`ClientErrorEnum::Request`] that could not be built.
    pub fn request(error: ProtocolError) -> Self {
        Self(ClientErrorEnum::Request { error })
    }

    /// A [`ClientErrorEnum::ReplyMismatch`] between a reply and its request.
    pub fn reply_mismatch(field: &'static str, expected: u64, found: u64) -> Self {
        Self(ClientErrorEnum::ReplyMismatch {
            field,
            expected,
            found,
        })
    }

    /// A commit whose retries ran out: [`ClientErrorEnum::RetriesExhausted`].
    pub fn retries_exhausted(session: u64, request: u64, attempts: u32) -> Self {
        Self(ClientErrorEnum::RetriesExhausted {
            session,
            request,
            attempts,
        })
    }

    /// A retried commit refused as evicted: [`ClientErrorEnum::OutcomeUnknown`].
    pub fn outcome_unknown(session: u64, request: u64) -> Self {
        Self(ClientErrorEnum::OutcomeUnknown { session, request })
    }

    /// A commit refused while another is [`ClientErrorEnum::InDoubt`].
    pub fn in_doubt(session: u64, request: u64) -> Self {
        Self(ClientErrorEnum::InDoubt { session, request })
    }

    /// A resolve refused as [`ClientErrorEnum::NotInDoubt`].
    pub fn not_in_doubt(session: u64) -> Self {
        Self(ClientErrorEnum::NotInDoubt { session })
    }

    /// A commit refused as [`ClientErrorEnum::SessionBusy`].
    pub fn session_busy(session: u64) -> Self {
        Self(ClientErrorEnum::SessionBusy { session })
    }

    /// A request through an [`ClientErrorEnum::UnknownSession`].
    pub fn unknown_session(session: u64) -> Self {
        Self(ClientErrorEnum::UnknownSession { session })
    }

    /// A write refused as [`ClientErrorEnum::ReadOnly`].
    pub fn read_only(stream: u64) -> Self {
        Self(ClientErrorEnum::ReadOnly { stream })
    }

    /// A request on a stream refused as [`ClientErrorEnum::StreamFinished`].
    pub fn stream_finished(stream: u64) -> Self {
        Self(ClientErrorEnum::StreamFinished { stream })
    }

    /// A request on a stream refused as [`ClientErrorEnum::StreamBusy`].
    pub fn stream_busy(stream: u64) -> Self {
        Self(ClientErrorEnum::StreamBusy { stream })
    }

    /// A new stream refused at the [`ClientErrorEnum::StreamLimit`].
    pub fn stream_limit(limit: u32) -> Self {
        Self(ClientErrorEnum::StreamLimit { limit })
    }

    /// A read or execute refused as a [`ClientErrorEnum::TransactionMismatch`].
    pub fn transaction_mismatch(stream: u64, named: u64) -> Self {
        Self(ClientErrorEnum::TransactionMismatch { stream, named })
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

    /// Whether the connection is gone with this error, or the error came of its loss, so the next request reconnects.
    pub fn closes_connection(&self) -> bool {
        matches!(
            &self.0,
            ClientErrorEnum::Transport { .. }
                | ClientErrorEnum::Protocol { .. }
                | ClientErrorEnum::Closed { .. }
                | ClientErrorEnum::TimedOut
                | ClientErrorEnum::ConnectionLost { .. }
        )
    }
}
