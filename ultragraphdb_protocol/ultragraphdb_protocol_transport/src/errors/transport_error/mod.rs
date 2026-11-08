/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod transport_error_display;
mod transport_error_error;

use std::io;

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::ErrorCode;

use crate::types::address_rule::AddressRule;

/// Why a connection or a stream operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportErrorEnum {
    /// A received frame broke a validation rule; the connection is answered with an error frame and closed.
    Protocol {
        /// The rule the frame broke.
        error: ProtocolError,
    },
    /// The connection's stream failed.
    Io {
        /// The kind of the stream's error.
        kind: io::ErrorKind,
        /// The stream's error, as text.
        message: String,
    },
    /// The peer closed the connection between frames.
    Closed,
    /// The peer closed the connection `received` bytes into a frame of `expected`.
    Truncated {
        /// The bytes of the frame received.
        received: usize,
        /// The bytes the frame holds.
        expected: usize,
    },
    /// Every one of the pool's `buffers` is in use.
    PoolExhausted {
        /// The buffers the pool holds.
        buffers: usize,
    },
    /// A request named stream 0, which carries only the connection's own frames.
    StreamZero,
    /// Stream `stream` is already open.
    StreamOpen {
        /// The stream named.
        stream: u64,
    },
    /// The connection holds `limit` streams in flight, as many as Welcome allows.
    StreamLimit {
        /// The streams Welcome allows in flight.
        limit: u32,
    },
    /// Stream `stream` is not open: never opened, finished or cancelled.
    UnknownStream {
        /// The stream named.
        stream: u64,
    },
    /// The peer refused with an Error frame of `code` and `message`.
    Refused {
        /// The Error frame's code.
        code: ErrorCode,
        /// The Error frame's message.
        message: String,
    },
    /// A frame of command `found` arrived where `expected` was due.
    UnexpectedCommand {
        /// The command due.
        expected: u16,
        /// The command that arrived.
        found: u16,
    },
    /// The client's mechanism refused the server's message: `reason` says why, for the client.
    Authentication {
        /// Why the mechanism refused it.
        reason: String,
    },
    /// The frame of command `command` the server answers with could not be built: `error` says why, the frame's size
    /// against the largest frame for a frame too large. Nothing of it was sent.
    FrameNotBuilt {
        /// The command of the frame.
        command: u16,
        /// Why it could not be built.
        error: ProtocolError,
    },
    /// `address` names no carrier: `rule` is the rule it breaks.
    Address {
        /// The address as given.
        address: String,
        /// The rule it breaks.
        rule: AddressRule,
    },
}

/// Why a connection or a stream operation failed; `.0` classifies it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportError(pub TransportErrorEnum);

impl TransportError {
    /// A [`TransportErrorEnum::Protocol`] for `error`.
    #[must_use]
    pub fn protocol(error: ProtocolError) -> Self {
        Self(TransportErrorEnum::Protocol { error })
    }

    /// A [`TransportErrorEnum::Io`] for the stream's `error`.
    #[must_use]
    pub fn io(error: &io::Error) -> Self {
        Self(TransportErrorEnum::Io {
            kind: error.kind(),
            message: error.to_string(),
        })
    }

    /// A [`TransportErrorEnum::Closed`].
    #[must_use]
    pub fn closed() -> Self {
        Self(TransportErrorEnum::Closed)
    }

    /// A [`TransportErrorEnum::Truncated`] `received` bytes into a frame of `expected`.
    #[must_use]
    pub fn truncated(received: usize, expected: usize) -> Self {
        Self(TransportErrorEnum::Truncated { received, expected })
    }

    /// A [`TransportErrorEnum::PoolExhausted`] for a pool of `buffers`.
    #[must_use]
    pub fn pool_exhausted(buffers: usize) -> Self {
        Self(TransportErrorEnum::PoolExhausted { buffers })
    }

    /// A [`TransportErrorEnum::StreamZero`].
    #[must_use]
    pub fn stream_zero() -> Self {
        Self(TransportErrorEnum::StreamZero)
    }

    /// A [`TransportErrorEnum::StreamOpen`] for `stream`.
    #[must_use]
    pub fn stream_open(stream: u64) -> Self {
        Self(TransportErrorEnum::StreamOpen { stream })
    }

    /// A [`TransportErrorEnum::StreamLimit`] at `limit`.
    #[must_use]
    pub fn stream_limit(limit: u32) -> Self {
        Self(TransportErrorEnum::StreamLimit { limit })
    }

    /// A [`TransportErrorEnum::UnknownStream`] for `stream`.
    #[must_use]
    pub fn unknown_stream(stream: u64) -> Self {
        Self(TransportErrorEnum::UnknownStream { stream })
    }

    /// A [`TransportErrorEnum::Refused`] with `code` and `message`.
    pub fn refused(code: ErrorCode, message: impl Into<String>) -> Self {
        Self(TransportErrorEnum::Refused {
            code,
            message: message.into(),
        })
    }

    /// A [`TransportErrorEnum::UnexpectedCommand`]: `found` where `expected` was due.
    #[must_use]
    pub fn unexpected_command(expected: u16, found: u16) -> Self {
        Self(TransportErrorEnum::UnexpectedCommand { expected, found })
    }

    /// A [`TransportErrorEnum::FrameNotBuilt`]: the frame of `command` could not be built, as `error` says.
    #[must_use]
    pub fn frame_not_built(command: u16, error: ProtocolError) -> Self {
        Self(TransportErrorEnum::FrameNotBuilt { command, error })
    }

    /// A [`TransportErrorEnum::Address`] of `address`, breaking `rule`.
    #[must_use]
    pub fn address(address: &str, rule: AddressRule) -> Self {
        Self(TransportErrorEnum::Address {
            address: address.to_string(),
            rule,
        })
    }

    /// A [`TransportErrorEnum::Authentication`] saying `reason`.
    pub fn authentication(reason: impl Into<String>) -> Self {
        Self(TransportErrorEnum::Authentication {
            reason: reason.into(),
        })
    }
}
