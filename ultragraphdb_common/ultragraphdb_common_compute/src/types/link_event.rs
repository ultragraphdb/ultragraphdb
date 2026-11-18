/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{ErrorReply, ReferenceRegistration};

/// What a role connection said of a stream a session opened on it: a frame answering a request sent on the stream, read, whose
/// body kept its command's rules, since the link closes the connection on any other (`PROTOCOL.md` §2.6, §13).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkEvent {
    /// A frame to relay as it came, whole: a Result, or a Commit's or a Drop's reply.
    Frame(Vec<u8>),
    /// A Bind's answer: the version its transaction reads at, 0 for a Drop's.
    Bound(u64),
    /// A Write's or an Operation's Held.
    Held,
    /// A Reference's answer: its first registration.
    Registered(ReferenceRegistration),
    /// An Error: the refusal it holds.
    Refused(ErrorReply),
    /// The connection closed, or could not be opened, under the stream.
    Closed,
}
