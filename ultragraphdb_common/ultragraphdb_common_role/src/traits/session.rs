/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::future::Future;

use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::ErrorReply;

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

/// What one connection serves beyond its rules: the connection's machine hands it each request its endpoint takes.
/// Static dispatch: a role names its own session.
pub trait Session {
    /// The next step 0 of an authentication is about to run, its message `first`: what the exchange needs from elsewhere
    /// is fetched here, before the machine, which runs without waiting, looks it up.
    fn authenticating(&mut self, _first: &[u8]) -> impl Future<Output = ()> {
        std::future::ready(())
    }

    /// The answer to the request `frame` on `stream`. The frame is the session's until it is dropped: a session may hand
    /// it on whole, and the connection reads no further frame until this ends.
    fn answer(&mut self, stream: u64, frame: ReceivedFrame) -> impl Future<Output = Answer>;

    /// The next frame this session sends unasked, and its stream. Dropped whenever a request arrives first, so it loses
    /// nothing when dropped; pending for a session with nothing to send.
    fn next_push(&mut self) -> impl Future<Output = (u64, Vec<u8>)> {
        std::future::pending()
    }

    /// The peer cancelled `stream`.
    fn cancelled(&mut self, _stream: u64) {}
}
