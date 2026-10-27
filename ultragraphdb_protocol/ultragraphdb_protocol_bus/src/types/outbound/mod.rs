/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, VecDeque};

use ultragraphdb_protocol_wire::{Command, FLAG_LAST, HeaderView};

use crate::errors::bus_error::BusError;

/// Whether `frame`, whose header `push` read, is a Result frame, the frames credit meters.
fn is_result(frame: &[u8]) -> bool {
    HeaderView::new(frame).is_ok_and(|h| h.command() == Command::Result.number())
}

/// Whether `frame` is its stream's last.
fn is_last(frame: &[u8]) -> bool {
    HeaderView::new(frame).is_ok_and(|h| h.flags() & FLAG_LAST != 0)
}

/// One open stream: its credit for Result frames and the frames waiting to be sent.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct StreamState {
    credit: u32,
    queue: VecDeque<Vec<u8>>,
}

/// A server's frames for one connection, scheduled across its streams: each stream's frames leave in the order pushed,
/// streams take turns, a Result frame leaves only while its stream holds credit, and a stream closes once a frame
/// flagged [`ultragraphdb_protocol_wire::FLAG_LAST`] has left or it is cancelled. A step function over its inputs:
/// nothing here waits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outbound {
    max_streams: u32,
    initial_credit: u32,
    streams: BTreeMap<u64, StreamState>,
    turn: u64,
}

impl Outbound {
    /// The scheduler of a connection whose Welcome allowed `max_streams` in flight, each starting with
    /// `initial_credit`.
    pub fn new(max_streams: u32, initial_credit: u32) -> Self {
        Self {
            max_streams,
            initial_credit,
            streams: BTreeMap::new(),
            turn: 0,
        }
    }

    /// Opens `stream` for a request, refused as [`BusError::StreamZero`], [`BusError::StreamOpen`], or
    /// [`BusError::StreamLimit`] when as many streams as allowed are open.
    pub fn open(&mut self, stream: u64) -> Result<(), BusError> {
        if stream == 0 {
            return Err(BusError::StreamZero());
        }
        if self.streams.contains_key(&stream) {
            return Err(BusError::StreamOpen(stream));
        }
        if self.streams.len() as u64 >= u64::from(self.max_streams) {
            return Err(BusError::StreamLimit(self.max_streams));
        }
        self.streams.insert(
            stream,
            StreamState {
                credit: self.initial_credit,
                queue: VecDeque::new(),
            },
        );
        Ok(())
    }

    /// Queues `frame`, a whole frame of `stream`, refused as [`BusError::Protocol`] when it holds no header and
    /// [`BusError::UnknownStream`] when the stream is not open.
    pub fn push(&mut self, stream: u64, frame: Vec<u8>) -> Result<(), BusError> {
        HeaderView::new(&frame).map_err(BusError::Protocol)?;
        let state = self
            .streams
            .get_mut(&stream)
            .ok_or(BusError::UnknownStream(stream))?;
        state.queue.push_back(frame);
        Ok(())
    }

    /// Grants `stream` credit for `frames` more Result frames; ignored for a stream not open.
    pub fn grant(&mut self, stream: u64, frames: u32) {
        if let Some(state) = self.streams.get_mut(&stream) {
            state.credit = state.credit.saturating_add(frames);
        }
    }

    /// Cancels `stream`: its queued frames are dropped and nothing more of it is sent. Whether it was open.
    pub fn cancel(&mut self, stream: u64) -> bool {
        self.streams.remove(&stream).is_some()
    }

    /// The next frame to send and its stream, the streams taking turns in order of their ids; `None` when no stream
    /// has a frame it may send.
    pub fn next_frame(&mut self) -> Option<(u64, Vec<u8>)> {
        let sendable = |state: &StreamState| {
            state
                .queue
                .front()
                .is_some_and(|frame| !is_result(frame) || state.credit > 0)
        };
        let stream = self
            .streams
            .range(self.turn.saturating_add(1)..)
            .chain(self.streams.range(..=self.turn))
            .find(|(_, state)| sendable(state))
            .map(|(stream, _)| *stream)?;
        let state = self.streams.get_mut(&stream)?;
        let frame = state.queue.pop_front()?;
        if is_result(&frame) {
            state.credit -= 1;
        }
        if is_last(&frame) {
            self.streams.remove(&stream);
        }
        self.turn = stream;
        Some((stream, frame))
    }

    pub fn is_open(&self, stream: u64) -> bool {
        self.streams.contains_key(&stream)
    }

    /// The stream's credit, `None` when it is not open.
    pub fn credit(&self, stream: u64) -> Option<u32> {
        self.streams.get(&stream).map(|state| state.credit)
    }

    /// The frames queued on the stream; zero when it is not open.
    pub fn queued(&self, stream: u64) -> usize {
        self.streams
            .get(&stream)
            .map_or(0, |state| state.queue.len())
    }

    pub fn open_streams(&self) -> usize {
        self.streams.len()
    }
}
