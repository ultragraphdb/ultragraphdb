/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, VecDeque};

use ultragraphdb_protocol_wire::{Command, FLAG_LAST, Header, HeaderView};

use crate::errors::transport_error::TransportError;

/// Whether `frame`, whose header `push` read, is a Result or an Entries frame, the frames credit meters
/// (`PROTOCOL.md` §11.3).
fn is_metered(frame: &[u8]) -> bool {
    HeaderView::new(frame).is_ok_and(|h| {
        h.command() == Command::Result.number() || h.command() == Command::Entries.number()
    })
}

/// Whether `frame` is its stream's last.
fn is_last(frame: &[u8]) -> bool {
    HeaderView::new(frame).is_ok_and(|h| h.flags() & FLAG_LAST != 0)
}

/// What an open stream carries: a request's replies, a transaction, or a read of the transaction on its stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Role {
    #[default]
    Request,
    Transaction,
    Read(u64),
}

/// One open stream: its role, its credit for Result and Entries frames, the frames waiting to be sent, whether its sender
/// waits to hear there is room for its next frame, and whether a frame it waits on left since it was last told.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct StreamState {
    role: Role,
    credit: u32,
    queue: VecDeque<Vec<u8>>,
    wants_room: bool,
    left: bool,
}

/// Every frame a server sends on one connection, scheduled across its streams and numbered as it leaves: the
/// connection's own frames on stream 0, and the refusals of streams never opened, go first, in the order pushed; each
/// stream's frames leave in the order pushed, streams take turns, a Result or an Entries frame leaves only while its
/// stream holds credit, and a stream closes once a frame flagged [`ultragraphdb_protocol_wire::FLAG_LAST`] has left or
/// it is cancelled. A transaction's reads close with it. Each frame leaves stamped with its `sequence`, from 1. A step
/// function over its inputs: nothing here waits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outbound {
    max_streams: u32,
    initial_credit: u32,
    streams: BTreeMap<u64, StreamState>,
    connection: VecDeque<Vec<u8>>,
    turn: u64,
    sequence: u128,
}

impl Outbound {
    /// The scheduler of a connection whose Welcome allowed `max_streams` in flight, each starting with
    /// `initial_credit`.
    pub fn new(max_streams: u32, initial_credit: u32) -> Self {
        Self {
            max_streams,
            initial_credit,
            streams: BTreeMap::new(),
            connection: VecDeque::new(),
            turn: 0,
            sequence: 0,
        }
    }

    /// Queues `frame`, a whole frame of the connection's own on stream 0, or the refusal of a stream never opened:
    /// sent before every stream's frames, in the order pushed. Refused as [`TransportError::protocol`] when it holds
    /// no header.
    pub fn push_connection(&mut self, frame: Vec<u8>) -> Result<(), TransportError> {
        let _header: HeaderView<'_> = HeaderView::new(&frame).map_err(TransportError::protocol)?;
        self.connection.push_back(frame);
        Ok(())
    }

    /// Opens `stream` for a transaction, whose reads [`Outbound::open_read`] ties to it; refused as
    /// [`Outbound::open`] refuses.
    pub fn open_transaction(&mut self, stream: u64) -> Result<(), TransportError> {
        self.open_as(stream, Role::Transaction)
    }

    /// Opens `stream` for a read of the transaction on stream `transaction`, with the initial credit; refused as
    /// [`Outbound::open`] refuses, and as [`TransportError::unknown_stream`] naming `transaction` when no transaction
    /// is open on it.
    pub fn open_read(&mut self, stream: u64, transaction: u64) -> Result<(), TransportError> {
        if !self.is_transaction(transaction) {
            return Err(TransportError::unknown_stream(transaction));
        }
        self.open_as(stream, Role::Read(transaction))
    }

    /// Opens `stream` as [`Outbound::open`] does, starting with `credit` in place of the Welcome's, as a ComputeRead
    /// names (`PROTOCOL.md` §11.3).
    ///
    /// # Errors
    ///
    /// As [`Outbound::open`] refuses: [`TransportError::stream_zero`], [`TransportError::stream_open`], or
    /// [`TransportError::stream_limit`] when as many streams as allowed are open.
    pub fn open_with_credit(&mut self, stream: u64, credit: u32) -> Result<(), TransportError> {
        self.open_as(stream, Role::Request)?;
        if let Some(state) = self.streams.get_mut(&stream) {
            state.credit = credit;
        }
        Ok(())
    }

    /// Whether a transaction is open on `stream`.
    pub fn is_transaction(&self, stream: u64) -> bool {
        self.streams
            .get(&stream)
            .is_some_and(|state| state.role == Role::Transaction)
    }

    /// Closes `stream`, and with a transaction's its reads; nothing for a stream not open.
    fn close(&mut self, stream: u64) {
        let Some(state) = self.streams.remove(&stream) else {
            return;
        };
        if state.role == Role::Transaction {
            self.streams
                .retain(|_, read| read.role != Role::Read(stream));
        }
    }

    /// `frame` stamped with the connection's next sequence.
    fn stamped(&mut self, mut frame: Vec<u8>) -> Vec<u8> {
        self.sequence = self.sequence.saturating_add(1);
        // push and push_connection proved the header present, so the stamp holds.
        let stamped = Header::stamp_sequence(&mut frame, self.sequence);
        debug_assert!(
            stamped.is_ok(),
            "a queued frame holds a header: {stamped:?}"
        );
        frame
    }

    /// The sequence of the last frame sent; 0 before the first.
    pub fn sent(&self) -> u128 {
        self.sequence
    }

    /// Opens `stream` for a request, refused as [`TransportError::stream_zero`], [`TransportError::stream_open`], or
    /// [`TransportError::stream_limit`] when as many streams as allowed are open.
    pub fn open(&mut self, stream: u64) -> Result<(), TransportError> {
        self.open_as(stream, Role::Request)
    }

    fn open_as(&mut self, stream: u64, role: Role) -> Result<(), TransportError> {
        if stream == 0 {
            return Err(TransportError::stream_zero());
        }
        if self.streams.contains_key(&stream) {
            return Err(TransportError::stream_open(stream));
        }
        if self.streams.len() as u64 >= u64::from(self.max_streams) {
            return Err(TransportError::stream_limit(self.max_streams));
        }
        // contains_key refused an open stream above, so no state is replaced.
        let _replaced: Option<StreamState> = self.streams.insert(
            stream,
            StreamState {
                role,
                credit: self.initial_credit,
                queue: VecDeque::new(),
                wants_room: false,
                left: false,
            },
        );
        Ok(())
    }

    /// Queues `frame`, a whole frame of `stream`, refused as [`TransportError::protocol`] when it holds no header and
    /// [`TransportError::unknown_stream`] when the stream is not open. A Result or an Entries frame not its stream's last
    /// makes [`Outbound::take_ready`] report the stream once there is room for the next.
    pub fn push(&mut self, stream: u64, frame: Vec<u8>) -> Result<(), TransportError> {
        let _header: HeaderView<'_> = HeaderView::new(&frame).map_err(TransportError::protocol)?;
        let state = self
            .streams
            .get_mut(&stream)
            .ok_or(TransportError::unknown_stream(stream))?;
        state.wants_room |= is_metered(&frame) && !is_last(&frame);
        state.queue.push_back(frame);
        Ok(())
    }

    /// The streams with room for their next Result or Entries frame, in order of their ids: each that such a frame, not
    /// its last, was pushed on since it was last reported, whose queue is empty and which holds credit. A sender keeping
    /// to these builds a stream's next frame only once the one before has left and credit allows it.
    pub fn take_ready(&mut self) -> Vec<u64> {
        self.streams
            .iter_mut()
            .filter(|(_, state)| state.wants_room && state.queue.is_empty() && state.credit > 0)
            .map(|(stream, state)| {
                state.wants_room = false;
                state.left = false;
                *stream
            })
            .collect()
    }

    /// The streams a Result or Entries frame, not their last, left since they were last reported here or by
    /// [`Outbound::take_ready`], which is asked first: those with no room for their next, whose sender so learns the frame it
    /// waits on is gone, in order of their ids.
    pub fn take_left(&mut self) -> Vec<u64> {
        self.streams
            .iter_mut()
            .filter(|(_, state)| state.left)
            .map(|(stream, state)| {
                state.left = false;
                *stream
            })
            .collect()
    }

    /// Grants `stream` credit for `frames` more Result or Entries frames; ignored for a stream not open.
    pub fn grant(&mut self, stream: u64, frames: u32) {
        if let Some(state) = self.streams.get_mut(&stream) {
            state.credit = state.credit.saturating_add(frames);
        }
    }

    /// Cancels `stream`: its queued frames are dropped and nothing more of it is sent, and a transaction's reads end
    /// with it. Whether it was open.
    pub fn cancel(&mut self, stream: u64) -> bool {
        let was_open = self.is_open(stream);
        self.close(stream);
        was_open
    }

    /// The next frame of the connection's own queue, stream 0's and the refusals of streams never opened, stamped
    /// with the connection's next sequence; `None` when that queue is empty, whatever the streams hold.
    pub fn next_connection_frame(&mut self) -> Option<(u64, Vec<u8>)> {
        let frame = self.connection.pop_front()?;
        let stream = HeaderView::new(&frame).map_or(0, |h| h.stream());
        Some((stream, self.stamped(frame)))
    }

    /// The next frame to send and its stream, 0 for the connection's own, stamped with the connection's next sequence;
    /// the connection's frames first, then the streams taking turns in order of their ids; `None` when no frame may
    /// be sent.
    pub fn next_frame(&mut self) -> Option<(u64, Vec<u8>)> {
        if let Some(next) = self.next_connection_frame() {
            return Some(next);
        }
        let sendable = |state: &StreamState| {
            state
                .queue
                .front()
                .is_some_and(|frame| !is_metered(frame) || state.credit > 0)
        };
        let stream = self
            .streams
            .range(self.turn.saturating_add(1)..)
            .chain(self.streams.range(..=self.turn))
            .find(|(_, state)| sendable(state))
            .map(|(stream, _)| *stream)?;
        let state = self.streams.get_mut(&stream)?;
        let frame = state.queue.pop_front()?;
        if is_metered(&frame) {
            // `sendable` took a metered frame only while the stream held credit.
            debug_assert!(state.credit > 0);
            state.credit = state.credit.saturating_sub(1);
            state.left = true;
        }
        if is_last(&frame) {
            self.close(stream);
        }
        self.turn = stream;
        Some((stream, self.stamped(frame)))
    }

    /// Whether `stream` is open.
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

    /// The streams open.
    pub fn open_streams(&self) -> usize {
        self.streams.len()
    }
}
