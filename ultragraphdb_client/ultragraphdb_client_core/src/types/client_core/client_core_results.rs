/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Instant;

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::{Login, ReceivedFrame};
use ultragraphdb_protocol_wire::{BodyView, Cancel, Command, Credit, FLAG_LAST, ResultView};

use crate::errors::client_error::ClientError;
use crate::types::batch_poll::BatchPoll;
use crate::types::client_core::ClientCore;
use crate::types::client_core::client_core_receive::refusal;
use crate::types::result_stream::ResultStream;

/// What a result stream's frame holds.
enum Arrived<T> {
    /// A batch, as `read` read it, and whether it is the stream's last.
    Batch(T, bool),
    /// The server's refusal of the read.
    Refused(ClientError),
    /// A rule the frame breaks.
    Broken(ClientError),
}

/// `frame` of a result stream, read through its view: its batch given to `read`, its refusal, or the rule it breaks.
fn arrived<T>(frame: &ReceivedFrame, read: impl FnOnce(&ResultView<'_>) -> T) -> Arrived<T> {
    let view = match frame.view() {
        Ok(view) => view,
        Err(error) => return Arrived::Broken(ClientError::Protocol(error)),
    };
    if view.command() == Command::Error {
        return match refusal(&view) {
            Ok(refused) => Arrived::Refused(refused),
            Err(error) => Arrived::Broken(error),
        };
    }
    match ResultView::from_frame(&view) {
        Ok(batch) => Arrived::Batch(read(&batch), view.header().flags() & FLAG_LAST != 0),
        Err(error) => Arrived::Broken(ClientError::Protocol(error)),
    }
}

impl<L: Login> ClientCore<L> {
    /// The next batch of `stream`, read in place: `read` is given the batch's [`ResultView`], borrowing the frame's
    /// pooled buffer, and its value is the batch's; nothing is copied or decoded beyond what `read` asks for.
    /// [`BatchPoll::Done`] after the last, which ends the read's stream; [`BatchPoll::Wait`] while none has arrived,
    /// granting the stream [`crate::ClientConfig::credit`] more frames when its credit is spent. A refusal on the
    /// stream is its error and ends it, keeping the connection and the transaction; a frame breaking a rule gives the
    /// connection up ([`ClientError::Protocol`]). A read cut short by its connection's loss, before its last frame
    /// arrived, answers what the connection's requests answered, once: [`ClientError::ConnectionLost`] with its cause,
    /// or [`ClientError::TimedOut`]; it is not retried, as its transaction was aborted with the connection
    /// (`PROTOCOL.md` §11.4). A read whose last frame had arrived is read to its end. A read whose stream or
    /// transaction ended, or whose loss was answered, is finished ([`ClientError::StreamFinished`]).
    pub fn next_batch<T>(
        &mut self,
        stream: &ResultStream,
        now: Instant,
        read: impl FnOnce(&ResultView<'_>) -> T,
    ) -> Result<BatchPoll<T>, ClientError> {
        let id = stream.stream();
        let key = (stream.generation(), id);
        if let Some(lost) = self.lost_reads.remove(&key) {
            return Err(lost);
        }
        if let Some(inbox) = self.kept_reads.get_mut(&key) {
            let Some(frame) = inbox.pop_front() else {
                self.kept_reads.remove(&key);
                return Ok(BatchPoll::Done);
            };
            return match arrived(&frame, read) {
                Arrived::Batch(value, _) => Ok(BatchPoll::Batch(value)),
                Arrived::Refused(error) | Arrived::Broken(error) => {
                    self.kept_reads.remove(&key);
                    Err(error)
                }
            };
        }
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
        match arrived(&frame, read) {
            Arrived::Batch(value, last) => {
                if last {
                    self.streams.remove(&id);
                    self.done_reads.insert(id);
                }
                Ok(BatchPoll::Batch(value))
            }
            Arrived::Refused(error) => {
                self.streams.remove(&id);
                Err(error)
            }
            Arrived::Broken(error) => {
                self.streams.remove(&id);
                self.lose(ClientError::ConnectionLost(Some(error.clone())));
                Err(error)
            }
        }
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

    /// Cancels `stream`, a read: no further frame of it is read, and its transaction goes on. A read lost or kept with
    /// its connection is forgotten.
    pub fn cancel(&mut self, stream: ResultStream) {
        let id = stream.stream();
        let key = (stream.generation(), id);
        if self.lost_reads.remove(&key).is_some() || self.kept_reads.remove(&key).is_some() {
            return;
        }
        if stream.generation() != self.generation || self.streams.remove(&id).is_none() {
            return;
        }
        if let Ok(frame) = self.frame(&Cancel, id) {
            self.outbox.push_back(frame);
        }
    }
}
