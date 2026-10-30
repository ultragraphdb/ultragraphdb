/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::commit_request::CommitRequest;

/// What the commit step steps on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input<P> {
    /// A commit request: it waits for the next round, which closes at once when no round is syncing.
    Commit(CommitRequest<P>),
    /// A session registration under its ticket, for `principal`, the session's owner: it waits for the next round as a
    /// commit does, takes a commit version, which numbers the session, and is answered with it once its round is
    /// applied.
    Register { ticket: u64, principal: u64 },
    /// The sync of the round in flight completed: that round is to be applied and acknowledged, and the next round
    /// closes over the requests waiting.
    Synced,
    /// The sync of the round in flight failed: none of its transactions is acknowledged, and every later commit is
    /// refused until the node restarts.
    SyncFailed,
    /// The read version of the oldest open read-write transaction, or the watermark when none is open: the conflict
    /// history is pruned below it.
    OldestRead(u64),
}
