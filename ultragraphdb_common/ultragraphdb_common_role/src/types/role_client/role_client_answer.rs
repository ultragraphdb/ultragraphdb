/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cmp::Ordering;

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::{ReceivedFrame, TransportError};
use ultragraphdb_protocol_wire::{BodyView, Command, ErrorReplyView};

use super::RoleClient;
use crate::types::answered::Answered;

impl RoleClient {
    /// The next frame answering `request`, a request sent on the last stream this client opened: the one rule by which the
    /// side that dialled reads what answers a request (`PROTOCOL.md` §2.5, step 3; §2.6), for a request of one reply, each
    /// frame of a reply in several, and a stream the peer sends on until it ends.
    ///
    /// A frame on the request's stream, of a command `request` is answered by, is [`Answered::Frame`]. The peer's Error there
    /// is [`Answered::Refused`], and its Error on stream 0 [`Answered::ConnectionRefused`]; neither closes the connection. A
    /// frame on a stream below the request's is late, since the client takes its earlier streams as ended (§11.2): it is
    /// checked whole, dropped, and the next frame read.
    ///
    /// # Errors
    ///
    /// As reading fails, and [`TransportError::protocol`] naming the rule of a frame the client refuses: one breaking a
    /// frame-level rule, a late one included; one on stream 0 that is no Error, or on a stream above the request's, neither
    /// of which answers a request; a command `request` is not answered by; an Error whose body does not read. Every error
    /// closes the connection before it is returned, as the side that dialled refusing a frame does (§2.6, §13), so the
    /// client is given up.
    ///
    /// A future dropped at its read cuts the frame in hand, as the type says.
    pub async fn next_answer(&mut self, request: Command) -> Result<Answered, TransportError> {
        // An event loop: an answer, a refusal or the connection's end ends it (Ultra rule 2).
        loop {
            let read = self.connection.read_frame(&self.pool).await;
            let arrived = read.and_then(|frame| {
                self.arrived(request, frame)
                    .map_err(TransportError::protocol)
            });
            match arrived {
                Ok(Some(answer)) => return Ok(answer),
                // A late frame, checked whole and dropped: its buffer is free for the next read.
                Ok(None) => {}
                Err(refusal) => {
                    self.connection.shutdown().await;
                    return Err(refusal);
                }
            }
        }
    }

    /// What `frame`, read for `request` on the last stream this client opened, is: the answer it is or carries, or `None`
    /// for a late frame, checked whole (`PROTOCOL.md` §2.5, step 3).
    ///
    /// # Errors
    ///
    /// The rule the frame breaks, which the client refuses it for.
    fn arrived(
        &self,
        request: Command,
        frame: ReceivedFrame,
    ) -> Result<Option<Answered>, ProtocolError> {
        let view = frame.view()?;
        let stream = view.header().stream();
        let command = view.command();
        if stream == 0 {
            if command != Command::Error {
                return Err(ProtocolError::command_not_served(command.number()));
            }
            let error = ErrorReplyView::from_frame(&view)?;
            return Ok(Some(Answered::ConnectionRefused {
                code: error.code(),
                message: error.message().to_string(),
            }));
        }
        match stream.cmp(&self.stream) {
            Ordering::Less => Ok(None),
            Ordering::Greater => Err(ProtocolError::command_not_served(command.number())),
            Ordering::Equal if command == Command::Error => {
                let error = ErrorReplyView::from_frame(&view)?;
                Ok(Some(Answered::Refused {
                    code: error.code(),
                    message: error.message().to_string(),
                }))
            }
            Ordering::Equal => {
                let _answer: Command = command.answering(request)?;
                Ok(Some(Answered::Frame(frame)))
            }
        }
    }
}
