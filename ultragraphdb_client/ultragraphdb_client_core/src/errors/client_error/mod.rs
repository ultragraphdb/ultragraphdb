/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod client_error_display;
mod client_error_error;

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::TransportError;
use ultragraphdb_protocol_wire::{ErrorCode, RefusedRequest, RequestTerm, RunsAgain};

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
        /// The kind of request refused, which decides what its code runs again.
        request: RefusedRequest,
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
        expected: u128,
        /// The reply's value.
        found: u128,
    },
    /// Whether request `request` of session `session` was applied is unknown for good, and it is not retried: its resend
    /// was refused as evicted (code 12), the session gone from the client as from the server, or as out of order (code
    /// 13), the session's counter moved above the partition's last; or it went through no session, `session` and
    /// `request` 0, and lost its reply or was answered code 17.
    OutcomeUnknown {
        /// The evicted session.
        session: u128,
        /// The request number of the retried commit.
        request: u64,
    },
    /// Session `session` has the commit of `request` in doubt: it was sent, or answered code 17, and may have been
    /// applied. The session resolves it by a resend, with the same request number, before it commits again.
    InDoubt {
        /// The session with the commit in doubt.
        session: u128,
        /// The request number of the commit in doubt.
        request: u64,
        /// What left it in doubt, if anything said: the refusal, or why a retry could not be sent.
        cause: Option<Box<ClientError>>,
    },
    /// Session `session` has no commit in doubt to resolve.
    NotInDoubt {
        /// The session asked to resolve.
        session: u128,
    },
    /// Session `session` already has a commit in flight: one at a time per session.
    SessionBusy {
        /// The busy session.
        session: u128,
    },
    /// The client holds no session `session`.
    UnknownSession {
        /// The session asked for.
        session: u128,
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
    #[must_use]
    pub fn transport(error: TransportError) -> Self {
        Self(ClientErrorEnum::Transport { error })
    }

    /// A [`ClientErrorEnum::Protocol`] violation by a frame the server sent.
    #[must_use]
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
            request: RefusedRequest::Other,
            code,
            detail,
            message: message.into(),
            value,
        })
    }

    /// A [`ClientErrorEnum::Closed`] connection, closed by the server's Error frame.
    #[must_use]
    pub fn closed(code: ErrorCode, message: impl Into<String>) -> Self {
        Self(ClientErrorEnum::Closed {
            code,
            message: message.into(),
        })
    }

    /// A [`ClientErrorEnum::TimedOut`] connection.
    #[must_use]
    pub fn timed_out() -> Self {
        Self(ClientErrorEnum::TimedOut)
    }

    /// A [`ClientErrorEnum::ConnectionLost`] request, with what gave its connection up.
    #[must_use]
    pub fn connection_lost(cause: Option<ClientError>) -> Self {
        Self(ClientErrorEnum::ConnectionLost {
            cause: cause.map(Box::new),
        })
    }

    /// A request refused as [`ClientErrorEnum::NotConnected`].
    #[must_use]
    pub fn not_connected() -> Self {
        Self(ClientErrorEnum::NotConnected)
    }

    /// A [`ClientErrorEnum::Request`] that could not be built.
    #[must_use]
    pub fn request(error: ProtocolError) -> Self {
        Self(ClientErrorEnum::Request { error })
    }

    /// A [`ClientErrorEnum::ReplyMismatch`] between a reply and its request.
    pub fn reply_mismatch(
        field: &'static str,
        expected: impl Into<u128>,
        found: impl Into<u128>,
    ) -> Self {
        Self(ClientErrorEnum::ReplyMismatch {
            field,
            expected: expected.into(),
            found: found.into(),
        })
    }

    /// A commit whose outcome is unknown for good: [`ClientErrorEnum::OutcomeUnknown`].
    #[must_use]
    pub fn outcome_unknown(session: u128, request: u64) -> Self {
        Self(ClientErrorEnum::OutcomeUnknown { session, request })
    }

    /// A commit [`ClientErrorEnum::InDoubt`], or one refused while another is, with what left it so.
    #[must_use]
    pub fn in_doubt(session: u128, request: u64, cause: Option<ClientError>) -> Self {
        Self(ClientErrorEnum::InDoubt {
            session,
            request,
            cause: cause.map(Box::new),
        })
    }

    /// A resolve refused as [`ClientErrorEnum::NotInDoubt`].
    #[must_use]
    pub fn not_in_doubt(session: u128) -> Self {
        Self(ClientErrorEnum::NotInDoubt { session })
    }

    /// A commit refused as [`ClientErrorEnum::SessionBusy`].
    #[must_use]
    pub fn session_busy(session: u128) -> Self {
        Self(ClientErrorEnum::SessionBusy { session })
    }

    /// A request through an [`ClientErrorEnum::UnknownSession`].
    #[must_use]
    pub fn unknown_session(session: u128) -> Self {
        Self(ClientErrorEnum::UnknownSession { session })
    }

    /// A write refused as [`ClientErrorEnum::ReadOnly`].
    #[must_use]
    pub fn read_only(stream: u64) -> Self {
        Self(ClientErrorEnum::ReadOnly { stream })
    }

    /// A request on a stream refused as [`ClientErrorEnum::StreamFinished`].
    #[must_use]
    pub fn stream_finished(stream: u64) -> Self {
        Self(ClientErrorEnum::StreamFinished { stream })
    }

    /// A request on a stream refused as [`ClientErrorEnum::StreamBusy`].
    #[must_use]
    pub fn stream_busy(stream: u64) -> Self {
        Self(ClientErrorEnum::StreamBusy { stream })
    }

    /// A new stream refused at the [`ClientErrorEnum::StreamLimit`].
    #[must_use]
    pub fn stream_limit(limit: u32) -> Self {
        Self(ClientErrorEnum::StreamLimit { limit })
    }

    /// A read or execute refused as a [`ClientErrorEnum::TransactionMismatch`].
    #[must_use]
    pub fn transaction_mismatch(stream: u64, named: u64) -> Self {
        Self(ClientErrorEnum::TransactionMismatch { stream, named })
    }

    /// The [`ClientErrorEnum::Refused`] request of the kind `kind` names; any other error unchanged.
    #[must_use]
    pub fn of_request(mut self, kind: RefusedRequest) -> Self {
        if let ClientErrorEnum::Refused { request, .. } = &mut self.0 {
            *request = kind;
        }
        self
    }

    /// What runs again, exactly where the wire says so: for a refusal its code's answer for its request
    /// ([`ErrorCode::runs_again`]), for a commit in doubt [`RunsAgain::InDoubt`], and for any other error nothing.
    #[must_use]
    pub fn runs_again(&self) -> Option<RunsAgain> {
        if let ClientErrorEnum::Refused { request, code, .. } = &self.0 {
            return code.runs_again(*request);
        }
        matches!(&self.0, ClientErrorEnum::InDoubt { .. }).then_some(RunsAgain::InDoubt)
    }

    /// Whether the application runs something again, as [`ClientError::runs_again`] names it; a commit in doubt runs
    /// nothing until it is resolved.
    #[must_use]
    pub fn retryable(&self) -> bool {
        self.runs_again()
            .is_some_and(|runs| runs != RunsAgain::InDoubt)
    }

    /// Whether a prepared statement was refused as stale: its tenant's catalog changed, and preparing it again
    /// succeeds.
    #[must_use]
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
    #[must_use]
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
