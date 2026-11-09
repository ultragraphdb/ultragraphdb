/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_sequencer::Close;

use crate::types::commit_request::CommitRequest;

/// What the commit step steps on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input<P> {
    /// A commit request: it waits for the next round, which closes at once when no round is syncing and the partition
    /// may stamp it, unless the caller holds rounds while it hands over several ([`Input::Hold`]).
    Commit(CommitRequest<P>),
    /// A registration the partition accepted as an entity's home, under its ticket, its write-set record of kind 2
    /// built by the home: it waits for the next round as a commit does, is admitted without validation and takes a
    /// commit version of its own, and is answered with it once its round is applied.
    Registration {
        /// The caller's ticket, which the answer carries back.
        ticket: u64,
        /// The registration's write-set record, kind 2.
        record: Vec<u8>,
        /// The dictionary entries the home created for the registration's entity, each a key of the dictionary
        /// keyspace and its value, which its round's dictionary section holds (`PROTOCOL.md` §12.1), so replay writes
        /// them as the round's apply did.
        dictionary: Vec<(Vec<u8>, Vec<u8>)>,
    },
    /// A close of the sequencer, delivered to the partition: the epoch after it opens, the catalog versions it announces
    /// take effect there, the sessions it evicts are purged from the session table, and the commits held for it are
    /// stamped in a round of the new epoch.
    Close(Close),
    /// The caller hands over several requests together, as a partition's core does with what one turn takes: until
    /// [`Input::Release`], a request arriving with no round in flight waits rather than closing one, so they share a round.
    Hold,
    /// The requests handed over together have arrived: the next round closes over what waits, when none is in flight.
    Release,
    /// The commit step's clock moved on, with nothing else to step on: a close request unanswered for
    /// `close_request_ms` refuses the commits held as busy.
    Tick,
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
