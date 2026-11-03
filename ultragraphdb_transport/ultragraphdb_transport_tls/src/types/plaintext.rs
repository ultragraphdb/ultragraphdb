/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a [`TlsSession`](crate::TlsSession) had of the peer's plaintext for a read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plaintext {
    /// This many bytes, at least one, were copied out.
    Read(usize),
    /// None yet: the session needs more records from its carrier.
    Pending,
    /// The peer ended the session with its `close_notify` alert; nothing more comes.
    Closed,
    /// The carrier closed without the peer's `close_notify`, so what was sent may have been cut short.
    Truncated,
}
