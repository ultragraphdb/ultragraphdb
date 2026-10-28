/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Instant;

use ultragraphdb_protocol_wire::{
    BodyView, Cancel, Command, Credit, FLAG_LAST, FrameView, ResultView,
};

use crate::errors::client_error::ClientError;
use crate::types::batch_poll::BatchPoll;
use crate::types::client_core::ClientCore;
use crate::types::client_core::client_core_receive::refusal;
use crate::types::result_stream::ResultStream;

impl ClientCore {
    /// The next batch of `stream`, read in place: `read` is given the batch's [`ResultView`], borrowing the frame's
    /// pooled buffer, and its value is the batch's; nothing is copied or decoded beyond what `read` asks for.
    /// [`BatchPoll::Done`] after the last; [`BatchPoll::Wait`] while none has arrived, granting the stream
    /// [`crate::ClientConfig::credit`] more frames when its credit is spent. A refusal on the stream is its error and
    /// ends it, keeping the connection; a frame breaking a rule gives the connection up ([`ClientError::Protocol`]).
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
            return Err(ClientError::StreamFinished(id));
        };
        if let Some(failed) = state.failed.take() {
            self.streams.remove(&id);
            return Err(failed);
        }
        if !state.results {
            return Ok(BatchPoll::Done);
        }
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
        let cluster = self.config.cluster();
        let view = match FrameView::validate(frame.bytes(), cluster) {
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
        if view.header().flags() & FLAG_LAST != 0
            && let Some(state) = self.streams.get_mut(&id)
        {
            state.results = false;
            state.inbox.clear();
        }
        Ok(BatchPoll::Batch(value))
    }

    /// Grants `stream` credit for `frames` more Result frames, beyond what [`ClientCore::next_batch`] grants.
    pub fn grant(&mut self, stream: &ResultStream, frames: u32) -> Result<(), ClientError> {
        let id = stream.stream();
        let open = stream.generation() == self.generation
            && self.streams.get(&id).is_some_and(|s| s.results);
        if !open {
            return Err(ClientError::StreamFinished(id));
        }
        let frame = self.frame(&Credit::new(frames), id)?;
        if let Some(state) = self.streams.get_mut(&id) {
            state.credit = state.credit.saturating_add(frames);
        }
        self.outbox.push_back(frame);
        Ok(())
    }

    /// Cancels `stream`: no further frame of it is read, and its transaction ends with it, since a transaction is
    /// its stream.
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
