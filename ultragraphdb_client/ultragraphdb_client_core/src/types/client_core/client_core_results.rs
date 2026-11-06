/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::VecDeque;
use std::num::NonZeroU32;
use std::time::Instant;

use ultragraphdb_protocol_transport::{Login, ReceivedFrame};
use ultragraphdb_protocol_wire::{
    BodyView, Cancel, Command, Credit, FLAG_LAST, RefusedRequest, ResultView,
};

use crate::errors::client_error::ClientError;
use crate::types::batch_poll::BatchPoll;
use crate::types::client_core::ClientCore;
use crate::types::client_core::client_core_receive::refusal;
use crate::types::result_stream::ResultStream;
use crate::types::stream_state::StreamState;

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
        Err(error) => return Arrived::Broken(ClientError::protocol(error)),
    };
    if view.command() == Command::Error {
        return match refusal(&view, RefusedRequest::Read) {
            Ok(refused) => Arrived::Refused(refused),
            Err(error) => Arrived::Broken(error),
        };
    }
    // Every read this client sends is of a quad pattern, whose batch holds its rules.
    let batch = ResultView::from_frame(&view).and_then(|batch| {
        batch.check_pattern()?;
        Ok(batch)
    });
    match batch {
        Ok(batch) => Arrived::Batch(read(&batch), view.header().flags() & FLAG_LAST != 0),
        Err(error) => Arrived::Broken(ClientError::protocol(error)),
    }
}

impl<L: Login> ClientCore<L> {
    /// The next batch of `stream`, read in place: `read` is given the batch's [`ResultView`], borrowing the frame's
    /// pooled buffer, and its value is the batch's; nothing is copied or decoded beyond what `read` asks for.
    /// [`BatchPoll::Done`] after the last, which ends the read's stream; [`BatchPoll::Wait`] while none has arrived,
    /// granting the stream [`crate::ClientConfig::credit`] more frames when its credit is spent. A refusal on the
    /// stream is its error and ends it, keeping the connection and the transaction: only a stream ending in `LAST` is an
    /// answer, so the batches it handed over count for nothing, and no Credit or Cancel goes out for it and nothing of it is
    /// resent; a frame breaking a rule gives the
    /// connection up ([`ClientError::protocol`]). A read cut short by its connection's loss, before its last frame
    /// arrived, answers what the connection's requests answered, once: [`ClientError::connection_lost`] with its cause,
    /// or [`ClientError::timed_out`]; it is not retried, as its transaction was aborted with the connection
    /// (`PROTOCOL.md` §11.4). A read whose last frame had arrived is read to its end. A read whose stream or
    /// transaction ended, or whose loss was answered, is finished ([`ClientError::stream_finished`]).
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
                let _drained: Option<VecDeque<ReceivedFrame>> = self.kept_reads.remove(&key);
                return Ok(BatchPoll::Done);
            };
            return match arrived(&frame, read) {
                Arrived::Batch(value, _) => Ok(BatchPoll::Batch(value)),
                Arrived::Refused(error) | Arrived::Broken(error) => {
                    let _drained: Option<VecDeque<ReceivedFrame>> = self.kept_reads.remove(&key);
                    Err(error)
                }
            };
        }
        if stream.generation() != self.generation {
            return Err(ClientError::stream_finished(id));
        }
        let Some(state) = self.streams.get_mut(&id) else {
            if self.done_reads.contains(&id) {
                return Ok(BatchPoll::Done);
            }
            return Err(ClientError::stream_finished(id));
        };
        let Some(frame) = state.inbox.pop_front() else {
            // The read waits on the server from here: what its caller spent on the batch before counts as no silence.
            if std::mem::take(&mut state.with_caller) {
                state.progress = Some(now);
            }
            if state.credit == 0 {
                state.credit = self.config.credit();
                state.progress = Some(now);
                let credit = Credit::new(self.config.credit());
                let frame = self.frame(&credit, id)?;
                self.outbox.push_back(frame);
            }
            return Ok(BatchPoll::Wait);
        };
        match arrived(&frame, read) {
            Arrived::Batch(value, last) => {
                if last {
                    let _ended: Option<StreamState> = self.streams.remove(&id);
                    let _first_end: bool = self.done_reads.insert(id);
                } else {
                    state.with_caller = true;
                }
                Ok(BatchPoll::Batch(value))
            }
            Arrived::Refused(error) => {
                let _ended: Option<StreamState> = self.streams.remove(&id);
                Err(error)
            }
            Arrived::Broken(error) => {
                let _ended: Option<StreamState> = self.streams.remove(&id);
                self.lose(&ClientError::connection_lost(Some(error.clone())));
                Err(error)
            }
        }
    }

    /// Grants `stream` credit for `frames` more Result frames at `now`, beyond what [`ClientCore::next_batch`] grants; no
    /// shell offers it, since a shell's credit is automatic (`PROTOCOL.md` §11.3). A read whose end has arrived takes no
    /// Credit, since its stream has ended. Credit past what the read's frame buffers hold takes more of them, refused as
    /// [`ClientError::buffer_limit`] when the client has none left.
    pub fn grant(
        &mut self,
        stream: &ResultStream,
        frames: NonZeroU32,
        now: Instant,
    ) -> Result<(), ClientError> {
        let id = stream.stream();
        let open = stream.generation() == self.generation
            && self.streams.get(&id).is_some_and(|s| s.results);
        if !open {
            return Err(ClientError::stream_finished(id));
        }
        if self.streams.get(&id).is_some_and(StreamState::ended) {
            return Ok(());
        }
        let needed = self.streams.get(&id).map_or(0, |s| {
            u32::try_from(s.inbox.len())
                .unwrap_or(u32::MAX)
                .saturating_add(s.credit)
                .saturating_add(frames.get())
                .saturating_add(1)
        });
        let held = self.streams.get(&id).map_or(0, |s| s.buffers);
        self.reserve_buffers(needed.saturating_sub(held))?;
        let frame = self.frame(&Credit::new(frames.get()), id)?;
        if let Some(state) = self.streams.get_mut(&id) {
            state.credit = state.credit.saturating_add(frames.get());
            state.buffers = state.buffers.max(needed);
            state.progress = Some(now);
        }
        self.outbox.push_back(frame);
        Ok(())
    }

    /// Cancels `stream`, a read: no further frame of it is read, and its transaction goes on. A read lost or kept with
    /// its connection is forgotten, and one whose end has arrived sends no Cancel, since its stream has ended.
    pub fn cancel(&mut self, stream: ResultStream) {
        let id = stream.stream();
        let key = (stream.generation(), id);
        if self.lost_reads.remove(&key).is_some() || self.kept_reads.remove(&key).is_some() {
            return;
        }
        if stream.generation() != self.generation {
            return;
        }
        let Some(state) = self.streams.remove(&id) else {
            return;
        };
        if state.ended() {
            return;
        }
        if let Ok(frame) = self.frame(&Cancel, id) {
            self.outbox.push_back(frame);
        }
    }
}
