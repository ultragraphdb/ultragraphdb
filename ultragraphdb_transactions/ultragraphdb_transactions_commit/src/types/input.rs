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
