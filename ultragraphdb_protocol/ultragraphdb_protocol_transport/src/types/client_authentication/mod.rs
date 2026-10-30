/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{
    Authenticate, AuthenticateView, BodyView, Command, ErrorReplyView, FrameView,
};

use crate::errors::transport_error::TransportError;
use crate::traits::client_mechanism::ClientMechanism;

/// What a client's exchange does after a server message: send its next message, or stop, authenticated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthenticationStep {
    /// The client's next Authenticate, to send on stream 0.
    Send(Authenticate),
    /// The server's final message, checked: the connection acts for `principal` until `expires`, microseconds of
    /// Unix time, 0 for never.
    Done { principal: u64, expires: u64 },
}

/// The client's side of an Authenticate exchange on stream 0, sans-I/O: the mechanism's messages numbered and carried
/// in Authenticate bodies, the server's read back, until its final message names the principal and the expiry. One
/// exchange per value; re-authentication runs a new one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientAuthentication<M> {
    mechanism: M,
    due: Option<u32>,
}

impl<M: ClientMechanism> ClientAuthentication<M> {
    pub fn new(mechanism: M) -> Self {
        Self {
            mechanism,
            due: None,
        }
    }

    /// The client's first Authenticate, step 0, naming the mechanism; refused as [`TransportError::Authentication`]
    /// when the mechanism cannot start.
    pub fn start(&mut self) -> Result<Authenticate, TransportError> {
        let message = self
            .mechanism
            .start()
            .map_err(|e| TransportError::Authentication(e.to_string()))?;
        self.due = Some(1);
        Ok(Authenticate::new(
            0,
            self.mechanism.name().to_string(),
            message,
        ))
    }

    /// The server's frame on stream 0 read: its next Authenticate answered, its final one checked, its Error refused
    /// as [`TransportError::Refused`]; a message out of step as [`TransportError::Protocol`], another command as
    /// [`TransportError::UnexpectedCommand`], and the mechanism's refusal as [`TransportError::Authentication`].
    pub fn receive(&mut self, frame: &FrameView<'_>) -> Result<AuthenticationStep, TransportError> {
        match frame.command() {
            Command::Error => {
                let error = ErrorReplyView::from_frame(frame).map_err(TransportError::Protocol)?;
                Err(TransportError::Refused(error.code(), error.message()))
            }
            Command::Authenticate => {
                let message =
                    AuthenticateView::from_frame(frame).map_err(TransportError::Protocol)?;
                if self.due != Some(message.step()) {
                    return Err(TransportError::Protocol(ProtocolError::FieldValue(
                        Command::Authenticate.number(),
                        "step",
                        message.step().into(),
                    )));
                }
                let refused = |e: M::Error| TransportError::Authentication(e.to_string());
                if message.is_final() {
                    self.due = None;
                    self.mechanism.finish(message.message()).map_err(refused)?;
                    return Ok(AuthenticationStep::Done {
                        principal: message.principal(),
                        expires: message.expires(),
                    });
                }
                let answer = self.mechanism.answer(message.message()).map_err(refused)?;
                let step = message.step().saturating_add(1);
                self.due = Some(step.saturating_add(1));
                Ok(AuthenticationStep::Send(Authenticate::new(
                    step, "", answer,
                )))
            }
            other => Err(TransportError::UnexpectedCommand(
                Command::Authenticate.number(),
                other.number(),
            )),
        }
    }

    pub fn mechanism(&self) -> &M {
        &self.mechanism
    }
}
