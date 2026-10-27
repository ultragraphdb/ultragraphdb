/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::bus_error::{BusError, BusErrorEnum};

impl fmt::Display for BusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            BusErrorEnum::Protocol { error } => write!(f, "frame refused: {error}"),
            BusErrorEnum::Io { message, .. } => write!(f, "connection failed: {message}"),
            BusErrorEnum::Closed => write!(f, "connection closed by the peer"),
            BusErrorEnum::Truncated { received, expected } => write!(
                f,
                "connection closed {received} bytes into a frame of {expected}"
            ),
            BusErrorEnum::PoolExhausted { buffers } => {
                write!(f, "all {buffers} frame buffers are in use")
            }
            BusErrorEnum::StreamZero => write!(f, "stream 0 carries no request"),
            BusErrorEnum::StreamOpen { stream } => write!(f, "stream {stream} is already open"),
            BusErrorEnum::StreamLimit { limit } => {
                write!(f, "{limit} streams are in flight, the limit")
            }
            BusErrorEnum::UnknownStream { stream } => write!(f, "stream {stream} is not open"),
            BusErrorEnum::Refused { code, message } => {
                write!(f, "refused with {code:?}: {message}")
            }
            BusErrorEnum::UnexpectedCommand { expected, found } => write!(
                f,
                "command {found} arrived where command {expected} was due"
            ),
        }
    }
}
