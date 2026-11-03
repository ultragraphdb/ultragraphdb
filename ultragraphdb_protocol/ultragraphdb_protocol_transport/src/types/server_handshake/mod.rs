/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, ErrorReply, FrameView, Header, HelloView, MAX_FRAME_SIZE,
    Welcome,
};

use crate::errors::transport_error::TransportError;
use crate::types::handshake_answer::HandshakeAnswer;

/// The server's side of the handshake, sans-I/O: the client's first frame read and answered as `server` answers a
/// Hello ([`Welcome::answer`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHandshake {
    cluster: u64,
    server: Welcome,
}

impl ServerHandshake {
    /// The handshake of a node of `cluster` welcoming clients as `server` does.
    pub fn new(cluster: u64, server: Welcome) -> Self {
        Self { cluster, server }
    }

    /// The header of the server's answer, its first frame on the connection: sequence 1.
    fn header(&self) -> Header {
        Header::new(self.cluster, Command::Welcome)
            .with_sequence(1)
            .with_release(self.server.release())
    }

    /// The protocol Error frame of `refusal`, answered before the connection closes; none when it cannot be built.
    fn refusal_frame(&self, error: &ErrorReply) -> Option<Vec<u8>> {
        error.frame(self.header()).ok()
    }

    /// The answer to the client's first frame, the bytes of a whole frame: a Hello within the server's protocols is
    /// answered with its Welcome; one outside them with its Error frame and refused as [`TransportError::refused`];
    /// another command as [`TransportError::unexpected_command`] and a frame breaking a rule, or not the client's
    /// sequence 1, as [`TransportError::protocol`], both answered with a protocol Error frame; a Welcome that cannot
    /// be framed, such as one past the largest frame, as [`TransportError::frame_not_built`], answered with nothing.
    pub fn receive(&self, frame: &[u8]) -> HandshakeAnswer {
        let refuse = |refusal: ProtocolError, error: TransportError| {
            HandshakeAnswer::new(
                self.refusal_frame(&ErrorReply::protocol(&refusal)),
                Err(error),
            )
        };
        // A client's first frame: its view and replica are 0 besides the header's other rules.
        let view = match FrameView::validate_client(frame, self.cluster, MAX_FRAME_SIZE) {
            Ok(view) => view,
            Err(refusal) => return refuse(refusal.clone(), TransportError::protocol(refusal)),
        };
        let sequence = view.header().sequence();
        if sequence != 1 {
            let refusal = ProtocolError::sequence_order(1, sequence);
            return refuse(refusal.clone(), TransportError::protocol(refusal));
        }
        if view.command() != Command::Hello {
            let found = view.command().number();
            return refuse(
                ProtocolError::wrong_command(Command::Hello.number(), found),
                TransportError::unexpected_command(Command::Hello.number(), found),
            );
        }
        let hello = match HelloView::from_frame(&view) {
            Ok(hello) => hello,
            Err(refusal) => return refuse(refusal.clone(), TransportError::protocol(refusal)),
        };
        match self.server.answer(&hello) {
            Ok(welcome) => match welcome.frame(self.header()) {
                Ok(welcome_frame) => HandshakeAnswer::new(Some(welcome_frame), Ok(welcome)),
                // The server's own Welcome cannot be framed: nothing is sent, and the outcome names the command.
                Err(refusal) => HandshakeAnswer::new(
                    None,
                    Err(TransportError::frame_not_built(
                        Command::Welcome.number(),
                        refusal,
                    )),
                ),
            },
            Err(error) => HandshakeAnswer::new(
                self.refusal_frame(&error),
                Err(TransportError::refused(error.code(), error.message())),
            ),
        }
    }
}
