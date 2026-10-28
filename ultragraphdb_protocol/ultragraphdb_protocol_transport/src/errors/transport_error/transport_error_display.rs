/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::transport_error::{TransportError, TransportErrorEnum};

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            TransportErrorEnum::Protocol { error } => write!(f, "frame refused: {error}"),
            TransportErrorEnum::Io { message, .. } => write!(f, "connection failed: {message}"),
            TransportErrorEnum::Closed => write!(f, "connection closed by the peer"),
            TransportErrorEnum::Truncated { received, expected } => write!(
                f,
                "connection closed {received} bytes into a frame of {expected}"
            ),
            TransportErrorEnum::PoolExhausted { buffers } => {
                write!(f, "all {buffers} frame buffers are in use")
            }
            TransportErrorEnum::StreamZero => write!(f, "stream 0 carries no request"),
            TransportErrorEnum::StreamOpen { stream } => {
                write!(f, "stream {stream} is already open")
            }
            TransportErrorEnum::StreamLimit { limit } => {
                write!(f, "{limit} streams are in flight, the limit")
            }
            TransportErrorEnum::UnknownStream { stream } => {
                write!(f, "stream {stream} is not open")
            }
            TransportErrorEnum::Refused { code, message } => {
                write!(f, "refused with {code:?}: {message}")
            }
            TransportErrorEnum::UnexpectedCommand { expected, found } => write!(
                f,
                "command {found} arrived where command {expected} was due"
            ),
        }
    }
}
