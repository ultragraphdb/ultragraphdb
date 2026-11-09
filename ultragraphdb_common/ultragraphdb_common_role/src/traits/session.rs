/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::future::Future;

use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::ErrorReply;

use crate::types::parked_bytes::ParkedBytes;

/// What a [`Session`] made of a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// The reply's frame, flagged as its stream needs.
    Reply(Vec<u8>),
    /// No frame yet: the stream stays open for what [`Session::next_push`] sends on it.
    Open,
    /// No frame at all: the stream ends with the request.
    Done,
    /// The request refused on its stream.
    Refuse(ErrorReply),
}

/// What a [`Session`] sends on a stream unasked, through [`Session::next_push`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Push {
    /// A frame, flagged as its stream needs: one flagged last ends the stream once it has left.
    Frame(Vec<u8>),
    /// The stream's end with no frame, as [`Answer::Done`] ends it: a frame of it still queued, waiting for credit, is dropped.
    End,
    /// A refusal of a request on the stream, sent as [`Answer::Refuse`] is: an Error flagged last on the stream, which ends it, or
    /// on the connection's own queue for a stream it does not hold open.
    Refuse(ErrorReply),
}

/// What one connection serves beyond its rules: the connection's machine hands it each request its endpoint takes.
/// Static dispatch: a role names its own session.
pub trait Session {
    /// The next step 0 of an authentication is about to run, its message `first`: what the exchange needs from elsewhere
    /// is fetched here, before the machine, which runs without waiting, looks it up.
    fn authenticating(&mut self, _first: &[u8]) -> impl Future<Output = ()> {
        std::future::ready(())
    }

    /// The connection authenticated, or authenticated again, as `principal`.
    fn authenticated(&mut self, _principal: u64) {}

    /// The connection's authentication was refused, which closes it.
    fn authentication_refused(&mut self) {}

    /// The answer to the request `frame` on `stream`. The frame is the session's until it is dropped: a session may hand
    /// it on whole, and the connection reads no further frame until this ends, so a request waiting on something beyond the
    /// session's own turn answers [`Answer::Open`], its frame copied and counted in [`Session::parked`] and its answer pushed.
    fn answer(&mut self, stream: u64, frame: ReceivedFrame) -> impl Future<Output = Answer>;

    /// What this session holds past [`Session::answer`], at whose bound the connection stops reading and goes on sending
    /// (`PROTOCOL.md` §11.12): asked once, as the connection starts. A partition's holds its parked frames, a client connection's what
    /// it has on its way to a role. `None` for a session that holds nothing so: a compute process's and the frontend's role address,
    /// each of whose waits is that of its own request.
    fn parked(&self) -> Option<ParkedBytes> {
        None
    }

    /// The next push this session sends unasked, a frame or a stream's end, and its stream, in the order the session gives
    /// them. Dropped whenever a request arrives first, so it loses nothing when dropped; pending for a session with nothing to
    /// send.
    fn next_push(&mut self) -> impl Future<Output = (u64, Push)> {
        std::future::pending()
    }

    /// The peer cancelled `stream`.
    fn cancelled(&mut self, _stream: u64) {}

    /// The peer granted the open `stream` credit for `frames` more frames, which the connection has counted.
    fn credited(&mut self, _stream: u64, _frames: u32) {}

    /// The peer's Hello was answered, stating `max_frame`, the largest frame it takes.
    fn welcomed(&mut self, _max_frame: u32) {}

    /// The Result or Entries frame last sent on `stream` has left and the stream holds credit: there is room for its next.
    fn ready(&mut self, _stream: u64) {}

    /// The Result or Entries frame last sent on `stream` has left and the stream holds no credit for its next.
    fn left(&mut self, _stream: u64) {}
}
