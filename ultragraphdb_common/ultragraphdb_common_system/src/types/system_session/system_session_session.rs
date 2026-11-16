/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::future::Future;
use std::pin::pin;

use futures::StreamExt;
use futures::future::{Either, select};
use ultragraphdb_common_role::{Answer, Push, Session};
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_scram::ScramRelay;
use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{BodyBuilder, Command, ErrorReply, FLAG_LAST, Header};

use crate::types::system_session::{SystemSession, close_frame, role_named, served};

/// The server nonce part a session's reading of a client-first-message takes: the reading builds no message.
const READING_NONCE: &str = "name";

impl Session for SystemSession {
    /// The name the exchange names, which a role connection's role is once it authenticates.
    fn authenticating(&mut self, first: &[u8]) -> impl Future<Output = ()> {
        self.named = ScramRelay::new(READING_NONCE).first(first).ok();
        std::future::ready(())
    }

    fn authenticated(&mut self, _principal: u64) {
        self.role = self.named.as_deref().and_then(role_named);
    }

    fn welcomed(&mut self, max_frame: u32) {
        // Lossless: a `usize` is at least 32 bits on every target the node builds for.
        self.max_frame = max_frame as usize;
    }

    /// A dropped future leaves what [`SystemSession`] says.
    async fn answer(&mut self, stream: u64, received: ReceivedFrame) -> Answer {
        let view = match received.view() {
            Ok(view) => view,
            Err(rule) => return Answer::Refuse(ErrorReply::protocol(&rule)),
        };
        let frame = &view;
        let command = frame.command();
        if !served(self.role, command) {
            return Answer::Close(ProtocolError::command_not_served(command.number()));
        }
        // An if-chain over the commands the table lists: a `match` would name every other command of the protocol.
        let answer = if command == Command::Join {
            self.join(stream, frame).await
        } else if command == Command::Members {
            self.members(stream, frame).await
        } else if command == Command::Lease {
            self.lease(stream, frame).await
        } else if command == Command::Placement {
            self.placement(stream, frame).await
        } else if command == Command::Catalog {
            self.catalog(stream, frame).await
        } else if command == Command::Subscribe {
            self.subscribe(stream, frame).await
        } else if command == Command::CloseRequest {
            self.close_request(frame).await
        } else if command == Command::DefaultEntry {
            self.default_entry(stream, frame).await
        } else if command == Command::Report {
            self.report(stream, frame).await
        } else if command == Command::Challenge {
            self.challenge(stream, frame).await
        } else if command == Command::Verify {
            self.verify(stream, frame).await
        } else if command == Command::Session {
            self.session(stream, frame)
        } else if command == Command::Check {
            self.check(stream, frame).await
        } else if command == Command::Ended {
            Ok(self.ended(frame).await)
        } else if command == Command::Declare {
            self.declare(stream, frame)
        } else if command == Command::Defaults {
            self.defaults(stream, frame).await
        } else {
            // `served` passes no other command: the table has no role send it.
            return Answer::Close(ProtocolError::command_not_served(command.number()));
        };
        answer.unwrap_or_else(Answer::Refuse)
    }

    /// The next answer pushed later, or the next close of this connection's subscription. A close's cursor moves only once
    /// a close came back, and a wait dropped for a request that arrived first asks again, so a close is neither lost nor
    /// sent twice; an answer waits in its channel, which a dropped wait leaves as it was.
    async fn next_push(&mut self) -> (u64, Push) {
        // At most two turns: one that does not return ends the subscription, and the next waits on none (Ultra rule 2).
        loop {
            let cluster = self.shared.cluster.id();
            let subscription = self.subscription;
            let owner = &self.shared.owner;
            let pushed = self.pushed.next();
            let closing = async move {
                match subscription {
                    Some((stream, after)) => (stream, owner.next_close(after).await),
                    None => std::future::pending().await,
                }
            };
            let (stream, close) = match select(pin!(pushed), pin!(closing)).await {
                // The session holds a sender, so the channel never ends.
                Either::Left((Some(pushed), _)) => return pushed,
                Either::Left((None, _)) => return std::future::pending().await,
                Either::Right(((stream, close), _)) => (stream, close),
            };
            let close = match close {
                Ok(close) => close,
                Err(refusal) => {
                    // A subscriber too far behind for the closes held has nothing left to read here, and is told so on its
                    // stream, which this ends (`PROTOCOL.md` §13.3).
                    tracing::warn!(reason = refusal.message(), "a subscription ended");
                    self.subscription = None;
                    let header = Header::new(cluster, Command::Error)
                        .with_stream(stream)
                        .with_flags(FLAG_LAST);
                    match refusal.frame(header) {
                        Ok(frame) => return (stream, Push::Frame(frame)),
                        Err(error) => {
                            tracing::error!(%error, "a subscription's end could not be framed");
                            continue;
                        }
                    }
                }
            };
            match close_frame(cluster, stream, &close) {
                Ok(frame) => {
                    self.subscription = Some((stream, close.epoch().get()));
                    return (stream, Push::Frame(frame));
                }
                Err(error) => {
                    tracing::error!(%error, "a close could not be framed");
                    self.subscription = None;
                }
            }
        }
    }

    fn cancelled(&mut self, stream: u64) {
        if self.subscription.is_some_and(|(held, _)| held == stream) {
            self.subscription = None;
        }
    }
}
