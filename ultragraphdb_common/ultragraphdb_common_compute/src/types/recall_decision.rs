/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::ErrorReply;

/// What the frontend answers a retried Commit once every partition answered its Recall (`PROTOCOL.md` §11.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecallDecision {
    /// The stored reply a partition gave.
    Stored,
    /// The refusal the answers call for.
    Refused(ErrorReply),
    /// Every partition answered 18: a second Check of kind 1 decides.
    SecondCheck,
}
