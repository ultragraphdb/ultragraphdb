/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What one partition answered a Recall of a retried Commit (`PROTOCOL.md` §11.5, *The in-doubt resend*).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecallAnswer {
    /// The stored reply: the request committed there.
    Stored,
    /// Code 14: the request is in its commit step.
    Busy,
    /// Code 17, any refusal not named here, or the role connection closed or not opened before it answered.
    Unavailable,
    /// Code 13, `detail` the partition's last request for the session.
    OutOfOrder(u64),
    /// Code 18: the partition holds neither.
    NotCommitted,
}
