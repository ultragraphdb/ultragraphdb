/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{Command, ErrorCode};

/// What a [`crate::ServerConnection`] made of a frame it received, or of a deadline passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inbound {
    /// The Hello was answered with a Welcome at `protocol`, which the connection's framer reads later frames at.
    Welcomed { protocol: u16 },
    /// The connection's own frame, answered or applied: an Authenticate message, a Credit.
    Handled,
    /// The connection authenticated, or authenticated again, as `principal`.
    Authenticated { principal: u64 },
    /// A request for the server's session, on `stream`, the frame's; its stream is open, a read's tied to its
    /// transaction.
    Request { stream: u64, command: Command },
    /// The client cancelled `stream`: a read, which ended, or a transaction, which aborts with its reads.
    Cancelled { stream: u64, transaction: bool },
    /// A request refused by the connection's rules on its stream, which the refusal ended; the connection stays.
    Refused { stream: u64, code: ErrorCode },
    /// The connection closes: its last frame, an Error of `code` on stream 0, is queued, or none for a connection idle
    /// past its limit; nothing more is read.
    Closing { code: Option<ErrorCode> },
}
