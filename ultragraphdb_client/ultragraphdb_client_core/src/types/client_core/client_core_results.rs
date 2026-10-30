/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Instant;

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::Login;
use ultragraphdb_protocol_wire::{BodyView, Cancel, Command, Credit, FLAG_LAST, ResultView};

use crate::errors::client_error::ClientError;
use crate::types::batch_poll::BatchPoll;
use crate::types::client_core::ClientCore;
use crate::types::client_core::client_core_receive::refusal;
use crate::types::result_stream::ResultStream;

impl<L: Login> ClientCore<L> {
    /// The next batch of `stream`, read in place: `read` is given the batch's [`ResultView`], borrowing the frame's
    /// pooled buffer, and its value is the batch's; nothing is copied or decoded beyond what `read` asks for.
    /// [`BatchPoll::Done`] after the last, which ends the read's stream; [`BatchPoll::Wait`] while none has arrived,
    /// granting the stream [`crate::ClientConfig::credit`] more frames when its credit is spent. A refusal on the
    /// stream is its error and ends it, keeping the connection and the transaction; a frame breaking a rule gives the
    /// connection up ([`ClientError::Protocol`]). A read whose transaction ended is finished
    /// ([`ClientError::StreamFinished`]).
    pub fn next_batch<T>(
        &mut self,
        stream: &ResultStream,
        now: Instant,
        read: impl FnOnce(&ResultView<'_>) -> T,
    ) -> Result<BatchPoll<T>, ClientError> {
        let id = stream.stream();
        if stream.generation() != self.generation {
            return Err(ClientError::StreamFinished(id));
        }
        let Some(state) = self.streams.get_mut(&id) else {
            if self.done_reads.contains(&id) {
                return Ok(BatchPoll::Done);
            }
            return Err(ClientError::StreamFinished(id));
        };
        let Some(frame) = state.inbox.pop_front() else {
            if state.credit == 0 {
                state.credit = self.config.credit();
                let credit = Credit::new(self.config.credit());
                let frame = self.frame(&credit, id)?;
                self.outbox.push_back(frame);
                self.last_progress = Some(now);
            }
            return Ok(BatchPoll::Wait);
        };
        let view = match frame.view() {
            Ok(view) => view,
            Err(error) => {
                self.lose(ClientError::ConnectionLost());
                return Err(ClientError::Protocol(error));
            }
        };
        if view.command() == Command::Error {
            let error = refusal(&view);
            self.streams.remove(&id);
            return match error {
                Ok(refused) => Err(refused),
                Err(error) => {
                    self.lose(ClientError::ConnectionLost());
                    Err(error)
                }
            };
        }
        let batch = match ResultView::from_frame(&view) {
            Ok(batch) => batch,
            Err(error) => {
                self.lose(ClientError::ConnectionLost());
                return Err(ClientError::Protocol(error));
            }
        };
        let value = read(&batch);
        if view.header().flags() & FLAG_LAST != 0 {
            self.streams.remove(&id);
            self.done_reads.insert(id);
        }
        Ok(BatchPoll::Batch(value))
    }

    /// Grants `stream` credit for `frames` more Result frames, beyond what [`ClientCore::next_batch`] grants; refused
    /// as [`ClientError::Request`] for no frames, which a Credit cannot carry (`PROTOCOL.md` §9.2).
    pub fn grant(&mut self, stream: &ResultStream, frames: u32) -> Result<(), ClientError> {
        let id = stream.stream();
        let open = stream.generation() == self.generation
            && self.streams.get(&id).is_some_and(|s| s.results);
        if !open {
            return Err(ClientError::StreamFinished(id));
        }
        if frames == 0 {
            return Err(ClientError::Request(ProtocolError::FieldValue(
                Command::Credit.number(),
                "frames",
                0,
            )));
        }
        let frame = self.frame(&Credit::new(frames), id)?;
        if let Some(state) = self.streams.get_mut(&id) {
            state.credit = state.credit.saturating_add(frames);
        }
        self.outbox.push_back(frame);
        Ok(())
    }

    /// Cancels `stream`, a read: no further frame of it is read, and its transaction goes on.
    pub fn cancel(&mut self, stream: ResultStream) {
        let id = stream.stream();
        if stream.generation() != self.generation || self.streams.remove(&id).is_none() {
            return;
        }
        if let Ok(frame) = self.frame(&Cancel, id) {
            self.outbox.push_back(frame);
        }
    }
}
