/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod transport_error_display;
mod transport_error_error;

use std::io;

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::ErrorCode;

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
}

/// Why a connection or a stream operation failed; `.0` classifies it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportError(pub TransportErrorEnum);

impl TransportError {
    /// A [`TransportErrorEnum::Protocol`] for `error`.
    #[allow(non_snake_case)]
    pub fn Protocol(error: ProtocolError) -> Self {
        Self(TransportErrorEnum::Protocol { error })
    }

    /// A [`TransportErrorEnum::Io`] for the stream's `error`.
    #[allow(non_snake_case)]
    pub fn Io(error: &io::Error) -> Self {
        Self(TransportErrorEnum::Io {
            kind: error.kind(),
            message: error.to_string(),
        })
    }

    /// A [`TransportErrorEnum::Closed`].
    #[allow(non_snake_case)]
    pub fn Closed() -> Self {
        Self(TransportErrorEnum::Closed)
    }

    /// A [`TransportErrorEnum::Truncated`] `received` bytes into a frame of `expected`.
    #[allow(non_snake_case)]
    pub fn Truncated(received: usize, expected: usize) -> Self {
        Self(TransportErrorEnum::Truncated { received, expected })
    }

    /// A [`TransportErrorEnum::PoolExhausted`] for a pool of `buffers`.
    #[allow(non_snake_case)]
    pub fn PoolExhausted(buffers: usize) -> Self {
        Self(TransportErrorEnum::PoolExhausted { buffers })
    }

    /// A [`TransportErrorEnum::StreamZero`].
    #[allow(non_snake_case)]
    pub fn StreamZero() -> Self {
        Self(TransportErrorEnum::StreamZero)
    }

    /// A [`TransportErrorEnum::StreamOpen`] for `stream`.
    #[allow(non_snake_case)]
    pub fn StreamOpen(stream: u64) -> Self {
        Self(TransportErrorEnum::StreamOpen { stream })
    }

    /// A [`TransportErrorEnum::StreamLimit`] at `limit`.
    #[allow(non_snake_case)]
    pub fn StreamLimit(limit: u32) -> Self {
        Self(TransportErrorEnum::StreamLimit { limit })
    }

    /// A [`TransportErrorEnum::UnknownStream`] for `stream`.
    #[allow(non_snake_case)]
    pub fn UnknownStream(stream: u64) -> Self {
        Self(TransportErrorEnum::UnknownStream { stream })
    }

    /// A [`TransportErrorEnum::Refused`] with `code` and `message`.
    #[allow(non_snake_case)]
    pub fn Refused(code: ErrorCode, message: impl Into<String>) -> Self {
        Self(TransportErrorEnum::Refused {
            code,
            message: message.into(),
        })
    }

    /// A [`TransportErrorEnum::UnexpectedCommand`]: `found` where `expected` was due.
    #[allow(non_snake_case)]
    pub fn UnexpectedCommand(expected: u16, found: u16) -> Self {
        Self(TransportErrorEnum::UnexpectedCommand { expected, found })
    }

    /// A [`TransportErrorEnum::Authentication`] saying `reason`.
    #[allow(non_snake_case)]
    pub fn Authentication(reason: impl Into<String>) -> Self {
        Self(TransportErrorEnum::Authentication {
            reason: reason.into(),
        })
    }
}
