/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{Command, ErrorCode};

/// What a [`crate::ServerConnection`] made of a frame it received, or of a deadline passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inbound {
    /// The Hello was answered with a Welcome at `protocol`, which the connection's framer reads later frames at.
    Welcomed {
        /// The protocol version the Welcome names.
        protocol: u16,
        /// The largest frame the client takes, as its Hello states.
        max_frame: u32,
    },
    /// The connection's own frame, answered or applied: an Authenticate message, a Credit.
    Handled,
    /// The connection authenticated, or authenticated again, as `principal`.
    Authenticated {
        /// The principal authenticated.
        principal: u64,
    },
    /// A request for the server's session, on `stream`, the frame's; its stream is open, a read's tied to its
    /// transaction.
    Request {
        /// The request's stream.
        stream: u64,
        /// The request's command.
        command: Command,
    },
    /// The client cancelled `stream`: a read, which ended, or a transaction, which aborts with its reads.
    Cancelled {
        /// The stream cancelled.
        stream: u64,
        /// Whether the stream was a transaction's.
        transaction: bool,
    },
    /// A request refused by the connection's rules on its stream, which the refusal ended; the connection stays.
    Refused {
        /// The request's stream.
        stream: u64,
        /// The refusal's code.
        code: ErrorCode,
    },
    /// The connection closes: its last frame, an Error of `code` on stream 0, is queued, or none for a connection idle
    /// past its limit; nothing more is read.
    Closing {
        /// The code of the last frame's Error, or none for an idle connection.
        code: Option<ErrorCode>,
    },
}
