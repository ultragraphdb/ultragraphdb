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
    Protocol { error: ProtocolError },
    /// The connection's stream failed.
    Io {
        kind: io::ErrorKind,
        message: String,
    },
    /// The peer closed the connection between frames.
    Closed,
    /// The peer closed the connection `received` bytes into a frame of `expected`.
    Truncated { received: usize, expected: usize },
    /// Every one of the pool's `buffers` is in use.
    PoolExhausted { buffers: usize },
    /// A request named stream 0, which carries only the connection's own frames.
    StreamZero,
    /// Stream `stream` is already open.
    StreamOpen { stream: u64 },
    /// The connection holds `limit` streams in flight, as many as Welcome allows.
    StreamLimit { limit: u32 },
    /// Stream `stream` is not open: never opened, finished or cancelled.
    UnknownStream { stream: u64 },
    /// The peer refused with an Error frame of `code` and `message`.
    Refused { code: ErrorCode, message: String },
    /// A frame of command `found` arrived where `expected` was due.
    UnexpectedCommand { expected: u16, found: u16 },
}

/// Why a connection or a stream operation failed; `.0` classifies it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportError(pub TransportErrorEnum);

impl TransportError {
    #[allow(non_snake_case)]
    pub fn Protocol(error: ProtocolError) -> Self {
        Self(TransportErrorEnum::Protocol { error })
    }

    #[allow(non_snake_case)]
    pub fn Io(error: &io::Error) -> Self {
        Self(TransportErrorEnum::Io {
            kind: error.kind(),
            message: error.to_string(),
        })
    }

    #[allow(non_snake_case)]
    pub fn Closed() -> Self {
        Self(TransportErrorEnum::Closed)
    }

    #[allow(non_snake_case)]
    pub fn Truncated(received: usize, expected: usize) -> Self {
        Self(TransportErrorEnum::Truncated { received, expected })
    }

    #[allow(non_snake_case)]
    pub fn PoolExhausted(buffers: usize) -> Self {
        Self(TransportErrorEnum::PoolExhausted { buffers })
    }

    #[allow(non_snake_case)]
    pub fn StreamZero() -> Self {
        Self(TransportErrorEnum::StreamZero)
    }

    #[allow(non_snake_case)]
    pub fn StreamOpen(stream: u64) -> Self {
        Self(TransportErrorEnum::StreamOpen { stream })
    }

    #[allow(non_snake_case)]
    pub fn StreamLimit(limit: u32) -> Self {
        Self(TransportErrorEnum::StreamLimit { limit })
    }

    #[allow(non_snake_case)]
    pub fn UnknownStream(stream: u64) -> Self {
        Self(TransportErrorEnum::UnknownStream { stream })
    }

    #[allow(non_snake_case)]
    pub fn Refused(code: ErrorCode, message: impl Into<String>) -> Self {
        Self(TransportErrorEnum::Refused {
            code,
            message: message.into(),
        })
    }

    #[allow(non_snake_case)]
    pub fn UnexpectedCommand(expected: u16, found: u16) -> Self {
        Self(TransportErrorEnum::UnexpectedCommand { expected, found })
    }
}
