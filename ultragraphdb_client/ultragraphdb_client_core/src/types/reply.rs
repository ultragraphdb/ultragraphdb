/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::Describe;

use crate::types::committed::Committed;
use crate::types::session::Session;
use crate::types::statement::Statement;
use crate::types::transaction::Transaction;

/// The answer to a request with one reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    /// A Register's session.
    Registered(Session),
    /// A Begin's transaction.
    Begun(Transaction),
    /// A Commit's answer.
    Committed(Committed),
    /// A Drop's answer: the drop's commit version and request, as a Commit's, with no results.
    Dropped(Committed),
    /// A Prepare's statement.
    Prepared(Statement),
    /// A Describe's answer.
    Described(Describe),
}
