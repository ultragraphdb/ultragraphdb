/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, ErrorReply, FrameView, Header, HelloView, Welcome,
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

    fn header(&self) -> Header {
        Header::new(self.cluster, Command::Welcome).with_release(self.server.release())
    }

    /// The protocol Error frame of `refusal`, answered before the connection closes; none when it cannot be built.
    fn refusal_frame(&self, error: &ErrorReply) -> Option<Vec<u8>> {
        error.frame(self.header()).ok()
    }

    /// The answer to the client's first frame, the bytes of a whole frame: a Hello within the server's protocols is
    /// answered with its Welcome; one outside them with its Error frame and refused as [`TransportError::Refused`];
    /// another command as [`TransportError::UnexpectedCommand`] and a frame breaking a rule as
    /// [`TransportError::Protocol`], both answered with a protocol Error frame.
    pub fn receive(&self, frame: &[u8]) -> HandshakeAnswer {
        let refuse = |refusal: ProtocolError, error: TransportError| {
            HandshakeAnswer::new(
                self.refusal_frame(&ErrorReply::protocol(&refusal)),
                Err(error),
            )
        };
        let view = match FrameView::validate(frame, self.cluster) {
            Ok(view) => view,
            Err(refusal) => return refuse(refusal.clone(), TransportError::Protocol(refusal)),
        };
        if view.command() != Command::Hello {
            let found = view.command().number();
            return refuse(
                ProtocolError::WrongCommand(Command::Hello.number(), found),
                TransportError::UnexpectedCommand(Command::Hello.number(), found),
            );
        }
        let hello = match HelloView::from_frame(&view) {
            Ok(hello) => hello,
            Err(refusal) => return refuse(refusal.clone(), TransportError::Protocol(refusal)),
        };
        match self.server.answer(&hello) {
            Ok(welcome) => match welcome.frame(self.header()) {
                Ok(frame) => HandshakeAnswer::new(Some(frame), Ok(welcome)),
                Err(refusal) => HandshakeAnswer::new(None, Err(TransportError::Protocol(refusal))),
            },
            Err(error) => HandshakeAnswer::new(
                self.refusal_frame(&error),
                Err(TransportError::Refused(error.code(), error.message())),
            ),
        }
    }
}
