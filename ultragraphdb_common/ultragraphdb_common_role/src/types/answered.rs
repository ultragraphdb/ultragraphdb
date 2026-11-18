/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_transport::{ReceivedFrame, TransportError};
use ultragraphdb_protocol_wire::ErrorCode;

/// What [`crate::RoleClient::next_answer`] read for a request: the next frame its peer answers it with, or the peer's
/// Error (`PROTOCOL.md` §2.5, step 3; §2.6). A frame the client itself refuses is no answer: it closes the connection and
/// is the read's error.
#[derive(Debug)]
pub enum Answered {
    /// A frame on the request's stream, of a command the request is answered by. It holds the connection's one buffer
    /// until it is dropped, and its header's `FLAG_LAST` says whether the peer ended the stream with it.
    Frame(ReceivedFrame),
    /// The peer's Error on the request's stream, which ends the stream; the connection stays open.
    Refused {
        /// The Error frame's code.
        code: ErrorCode,
        /// The Error frame's message.
        message: String,
    },
    /// The peer's Error on stream 0, which is about the connection and no one stream: with code 1 a connection error,
    /// after which the peer closes the connection (§2.6).
    ConnectionRefused {
        /// The Error frame's code.
        code: ErrorCode,
        /// The Error frame's message.
        message: String,
    },
}

impl Answered {
    /// The answer's frame, for a caller that takes the peer's Error as its request's failure wherever it came.
    ///
    /// # Errors
    ///
    /// [`TransportError::refused`] with the code and message of the peer's Error, on the request's stream or on stream 0.
    pub fn frame(self) -> Result<ReceivedFrame, TransportError> {
        match self {
            Self::Frame(frame) => Ok(frame),
            Self::Refused { code, message } | Self::ConnectionRefused { code, message } => {
                Err(TransportError::refused(code, message))
            }
        }
    }
}
