/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::Welcome;

use crate::errors::transport_error::TransportError;

/// What a [`crate::ServerHandshake`] decided about a client's first frame: the frame to send back, if any (the
/// Welcome, or the Error the connection closes with), and the handshake's outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeAnswer {
    frame: Option<Vec<u8>>,
    outcome: Result<Welcome, TransportError>,
}

impl HandshakeAnswer {
    pub(crate) fn new(frame: Option<Vec<u8>>, outcome: Result<Welcome, TransportError>) -> Self {
        Self { frame, outcome }
    }

    /// The frame to send: the Welcome, or the Error frame before the connection closes.
    pub fn frame(&self) -> Option<&[u8]> {
        self.frame.as_deref()
    }

    /// The Welcome sent, or why the handshake failed.
    pub fn outcome(&self) -> &Result<Welcome, TransportError> {
        &self.outcome
    }

    /// The frame to send and the outcome, taken apart.
    pub fn into_parts(self) -> (Option<Vec<u8>>, Result<Welcome, TransportError>) {
        (self.frame, self.outcome)
    }
}
