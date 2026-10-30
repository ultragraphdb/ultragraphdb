/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, ErrorReplyView, FrameView, Header, Hello, MAX_FRAME_SIZE,
    Welcome, WelcomeView,
};

use crate::errors::transport_error::TransportError;

/// The client's side of the handshake, sans-I/O: the Hello frame to send on stream 0, then the server's answer read:
/// its Welcome at a protocol within the Hello's range, or the refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHandshake {
    cluster: u64,
    hello: Hello,
}

impl ClientHandshake {
    /// The handshake of a client saying `hello` to a node of `cluster`.
    pub fn new(cluster: u64, hello: Hello) -> Self {
        Self { cluster, hello }
    }

    pub fn hello(&self) -> &Hello {
        &self.hello
    }

    /// The Hello frame, the client's first: sequence 1, on stream 0, with the Hello's release.
    pub fn hello_frame(&self) -> Result<Vec<u8>, TransportError> {
        let header = Header::new(self.cluster, Command::Hello)
            .with_sequence(1)
            .with_release(self.hello.release());
        self.hello.frame(header).map_err(TransportError::Protocol)
    }

    /// The server's answer, the bytes of a whole frame: its Welcome; refused as [`TransportError::Refused`] for an
    /// Error frame, [`TransportError::UnexpectedCommand`] for another command, and [`TransportError::Protocol`] when
    /// the frame or the Welcome breaks a rule or the Welcome chose a protocol outside the Hello's range.
    pub fn receive(&self, frame: &[u8]) -> Result<Welcome, TransportError> {
        let view = FrameView::validate(frame, self.cluster, MAX_FRAME_SIZE)
            .map_err(TransportError::Protocol)?;
        match view.command() {
            Command::Welcome => {
                let welcome = WelcomeView::from_frame(&view)
                    .and_then(|w| w.to_body())
                    .map_err(TransportError::Protocol)?;
                if !(self.hello.protocol_min()..=self.hello.protocol_max())
                    .contains(&welcome.protocol())
                {
                    return Err(TransportError::Protocol(ProtocolError::FieldValue(
                        Command::Welcome.number(),
                        "protocol",
                        welcome.protocol().into(),
                    )));
                }
                Ok(welcome)
            }
            Command::Error => {
                let error = ErrorReplyView::from_frame(&view).map_err(TransportError::Protocol)?;
                Err(TransportError::Refused(error.code(), error.message()))
            }
            other => Err(TransportError::UnexpectedCommand(
                Command::Welcome.number(),
                other.number(),
            )),
        }
    }
}
