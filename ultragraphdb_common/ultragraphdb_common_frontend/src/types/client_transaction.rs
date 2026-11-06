/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeSet, VecDeque};
use std::time::Instant;

use ultragraphdb_protocol_wire::{Declared, ErrorReply, ReadLevel};

use crate::types::bind_state::BindState;
use crate::types::queued::Queued;

/// A transaction the connection holds open, by the stream of its Begin.
#[derive(Debug)]
pub(crate) struct ClientTransaction {
    pub tenant: u64,
    pub read_only: bool,
    pub level: ReadLevel,
    /// The version the client named.
    pub named: Option<u64>,
    /// The read version its Begin answered.
    pub read_version: u64,
    /// The snapshot's epoch at level 0, the token's else.
    pub epoch: u64,
    /// The highest epoch any read has answered at, a monotonic-fast read's floor.
    pub floor: u64,
    /// The catalog version current at its Begin, which its declarations build on.
    pub catalog: u64,
    /// When a read's wait ends.
    pub deadline: Instant,
    /// When a read-write transaction passes its limit.
    pub limit: Option<Instant>,
    pub identity: [u8; 16],
    pub bind: BindState,
    /// Which of its Binds is current.
    pub attempt: u32,
    /// Whether its first request has had its one resend.
    pub resent: bool,
    /// Where a resend must bind: the owner a refusal named.
    pub forced: Option<u32>,
    /// A refreshed map is awaited, and nothing is sent until it comes.
    pub awaiting_map: bool,
    /// What a first request's or a claim's answer awaits.
    pub awaiting_claim: bool,
    /// The partition a claim's answer named.
    pub claimed: Option<u32>,
    pub queue: VecDeque<Queued>,
    /// The Write on the Bind stream not yet answered by Held, whole while it is the first request, whose refusal resends it.
    pub write_out: Option<Vec<u8>>,
    /// Whether nothing has answered its first request yet, so no later one may be sent.
    pub first_open: bool,
    /// Whether a request has reached a partition or compute.
    pub first_sent: bool,
    /// The read that is its first request while it is unanswered.
    pub first_read: Option<u64>,
    /// The reads at a compute process, unanswered.
    pub reads_out: BTreeSet<u64>,
    /// The partition a level-1 transaction's first read named.
    pub level_partition: Option<u32>,
    /// A first read's code 23, waiting for the refreshed map to say whether its subject moved.
    pub crossed: Option<(u64, ErrorReply)>,
    /// The session of its Commit, once relayed.
    pub commit_out: Option<u128>,
    /// The session its Commit marked in flight at the owner, from the owner's check until the transaction ends, 0 for none: one
    /// release whatever ends it.
    pub session_held: u128,
    /// Why its next request and every later read is refused: a read's code 23 ended it while its stream stays open.
    pub ended: Option<ErrorReply>,
    /// The declarations its Writes held.
    pub declared: Vec<Declared>,
    /// Whether it changed or read data.
    pub touched: bool,
}

impl ClientTransaction {
    /// Whether its Commit has been taken, queued, relayed or declaring: what decides it, so a later Write, Operation or Commit on
    /// its stream is late (`PROTOCOL.md` §11.2).
    pub fn commit_taken(&self) -> bool {
        self.commit_out.is_some()
            || self
                .queue
                .iter()
                .any(|queued| matches!(queued, Queued::Commit { .. }))
    }
}
