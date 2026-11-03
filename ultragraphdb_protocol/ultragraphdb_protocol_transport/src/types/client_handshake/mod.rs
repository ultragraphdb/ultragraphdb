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

    /// The Hello the client says.
    pub fn hello(&self) -> &Hello {
        &self.hello
    }

    /// The Hello frame, the client's first: sequence 1, on stream 0, with the Hello's release.
    pub fn hello_frame(&self) -> Result<Vec<u8>, TransportError> {
        let header = Header::new(self.cluster, Command::Hello)
            .with_sequence(1)
            .with_release(self.hello.release());
        self.hello.frame(header).map_err(TransportError::protocol)
    }

    /// The server's answer, the bytes of a whole frame, the server's first: its Welcome; refused as
    /// [`TransportError::refused`] for an Error frame, [`TransportError::unexpected_command`] for another command, and
    /// [`TransportError::protocol`] when the frame or the Welcome breaks a rule, the frame is not the server's
    /// sequence 1, or the Welcome chose a protocol outside the Hello's range.
    pub fn receive(&self, frame: &[u8]) -> Result<Welcome, TransportError> {
        let view = FrameView::validate(frame, self.cluster, MAX_FRAME_SIZE)
            .map_err(TransportError::protocol)?;
        let sequence = view.header().sequence();
        if sequence != 1 {
            return Err(TransportError::protocol(ProtocolError::sequence_order(
                1, sequence,
            )));
        }
        match view.command() {
            Command::Welcome => {
                let welcome = WelcomeView::from_frame(&view)
                    .and_then(|w| w.to_body())
                    .map_err(TransportError::protocol)?;
                if !(self.hello.protocol_min()..=self.hello.protocol_max())
                    .contains(&welcome.protocol())
                {
                    return Err(TransportError::protocol(ProtocolError::field_value(
                        Command::Welcome.number(),
                        "protocol",
                        welcome.protocol().into(),
                    )));
                }
                Ok(welcome)
            }
            Command::Error => {
                let error = ErrorReplyView::from_frame(&view).map_err(TransportError::protocol)?;
                Err(TransportError::refused(error.code(), error.message()))
            }
            other @ (Command::Hello
            | Command::Register
            | Command::Begin
            | Command::Commit
            | Command::Abort
            | Command::Read
            | Command::Write
            | Command::Operation
            | Command::Prepare
            | Command::Execute
            | Command::Describe
            | Command::Result
            | Command::Credit
            | Command::Cancel
            | Command::Round
            | Command::Authenticate
            | Command::Drop
            | Command::Join
            | Command::Members
            | Command::Subscribe
            | Command::EpochClose
            | Command::Lease
            | Command::Placement
            | Command::Catalog
            | Command::Challenge
            | Command::Verify
            | Command::Bind
            | Command::Recall
            | Command::ComputeRead
            | Command::Scan
            | Command::Entries
            | Command::Reference
            | Command::CloseRequest
            | Command::Closed
            | Command::DefaultEntry
            | Command::Report) => Err(TransportError::unexpected_command(
                Command::Welcome.number(),
                other.number(),
            )),
        }
    }
}
