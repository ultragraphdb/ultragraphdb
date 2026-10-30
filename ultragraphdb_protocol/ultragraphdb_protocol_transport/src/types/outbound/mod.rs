/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, VecDeque};

use ultragraphdb_protocol_wire::{Command, FLAG_LAST, Header, HeaderView};

use crate::errors::transport_error::TransportError;

/// Whether `frame`, whose header `push` read, is a Result frame, the frames credit meters.
fn is_result(frame: &[u8]) -> bool {
    HeaderView::new(frame).is_ok_and(|h| h.command() == Command::Result.number())
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

/// One open stream: its role, its credit for Result frames and the frames waiting to be sent.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct StreamState {
    role: Role,
    credit: u32,
    queue: VecDeque<Vec<u8>>,
}

/// Every frame a server sends on one connection, scheduled across its streams and numbered as it leaves: the
/// connection's own frames on stream 0, and the refusals of streams never opened, go first, in the order pushed; each
/// stream's frames leave in the order pushed, streams take turns, a Result frame leaves only while its stream holds
/// credit, and a stream closes once a frame flagged [`ultragraphdb_protocol_wire::FLAG_LAST`] has left or it is
/// cancelled. A transaction's reads close with it. Each frame leaves stamped with its `sequence`, from 1. A step
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
    /// sent before every stream's frames, in the order pushed. Refused as [`TransportError::Protocol`] when it holds
    /// no header.
    pub fn push_connection(&mut self, frame: Vec<u8>) -> Result<(), TransportError> {
        HeaderView::new(&frame).map_err(TransportError::Protocol)?;
        self.connection.push_back(frame);
        Ok(())
    }

    /// Opens `stream` for a transaction, whose reads [`Outbound::open_read`] ties to it; refused as
    /// [`Outbound::open`] refuses.
    pub fn open_transaction(&mut self, stream: u64) -> Result<(), TransportError> {
        self.open_as(stream, Role::Transaction)
    }

    /// Opens `stream` for a read of the transaction on stream `transaction`, with the initial credit; refused as
    /// [`Outbound::open`] refuses, and as [`TransportError::UnknownStream`] naming `transaction` when no transaction
    /// is open on it.
    pub fn open_read(&mut self, stream: u64, transaction: u64) -> Result<(), TransportError> {
        if !self.is_transaction(transaction) {
            return Err(TransportError::UnknownStream(transaction));
        }
        self.open_as(stream, Role::Read(transaction))
    }

    /// Whether a transaction is open on `stream`.
    pub fn is_transaction(&self, stream: u64) -> bool {
        self.streams
            .get(&stream)
            .is_some_and(|state| state.role == Role::Transaction)
    }

    /// Closes `stream`, and with a transaction's its reads; whether it was open.
    fn close(&mut self, stream: u64) -> bool {
        let Some(state) = self.streams.remove(&stream) else {
            return false;
        };
        if state.role == Role::Transaction {
            self.streams
                .retain(|_, read| read.role != Role::Read(stream));
        }
        true
    }

    /// `frame` stamped with the connection's next sequence.
    fn stamped(&mut self, mut frame: Vec<u8>) -> Vec<u8> {
        self.sequence = self.sequence.saturating_add(1);
        // push and push_connection proved the header present, so the stamp holds.
        let _ = Header::stamp_sequence(&mut frame, self.sequence);
        frame
    }

    /// The sequence of the last frame sent; 0 before the first.
    pub fn sent(&self) -> u128 {
        self.sequence
    }

    /// Opens `stream` for a request, refused as [`TransportError::StreamZero`], [`TransportError::StreamOpen`], or
    /// [`TransportError::StreamLimit`] when as many streams as allowed are open.
    pub fn open(&mut self, stream: u64) -> Result<(), TransportError> {
        self.open_as(stream, Role::Request)
    }

    fn open_as(&mut self, stream: u64, role: Role) -> Result<(), TransportError> {
        if stream == 0 {
            return Err(TransportError::StreamZero());
        }
        if self.streams.contains_key(&stream) {
            return Err(TransportError::StreamOpen(stream));
        }
        if self.streams.len() as u64 >= u64::from(self.max_streams) {
            return Err(TransportError::StreamLimit(self.max_streams));
        }
        self.streams.insert(
            stream,
            StreamState {
                role,
                credit: self.initial_credit,
                queue: VecDeque::new(),
            },
        );
        Ok(())
    }

    /// Queues `frame`, a whole frame of `stream`, refused as [`TransportError::Protocol`] when it holds no header and
    /// [`TransportError::UnknownStream`] when the stream is not open.
    pub fn push(&mut self, stream: u64, frame: Vec<u8>) -> Result<(), TransportError> {
        HeaderView::new(&frame).map_err(TransportError::Protocol)?;
        let state = self
            .streams
            .get_mut(&stream)
            .ok_or(TransportError::UnknownStream(stream))?;
        state.queue.push_back(frame);
        Ok(())
    }

    /// Grants `stream` credit for `frames` more Result frames; ignored for a stream not open.
    pub fn grant(&mut self, stream: u64, frames: u32) {
        if let Some(state) = self.streams.get_mut(&stream) {
            state.credit = state.credit.saturating_add(frames);
        }
    }

    /// Cancels `stream`: its queued frames are dropped and nothing more of it is sent, and a transaction's reads end
    /// with it. Whether it was open.
    pub fn cancel(&mut self, stream: u64) -> bool {
        self.close(stream)
    }

    /// The next frame to send and its stream, 0 for the connection's own, stamped with the connection's next sequence;
    /// the connection's frames first, then the streams taking turns in order of their ids; `None` when no frame may
    /// be sent.
    pub fn next_frame(&mut self) -> Option<(u64, Vec<u8>)> {
        if let Some(frame) = self.connection.pop_front() {
            let stream = HeaderView::new(&frame).map_or(0, |h| h.stream());
            return Some((stream, self.stamped(frame)));
        }
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
            self.close(stream);
        }
        self.turn = stream;
        Some((stream, self.stamped(frame)))
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
