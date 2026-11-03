/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod sessions_store;

use std::collections::HashMap;

use crate::errors::commit_error::CommitError;
use crate::types::stored_reply::StoredReply;

/// One session's entry in a partition: its last request committed there and that commit's whole reply.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Session {
    last_request: u64,
    reply: StoredReply,
}

/// The bytes one entry accounts for: its 16-byte key, its request, version and token, and 16 for each result.
const ENTRY_BYTES: usize = 64;
const RESULT_BYTES: usize = 16;

/// A partition's session table (transactions, "Exactly-once commits through sessions"): keyed by the session's 128-bit
/// identity, each session that committed in the partition with its last request committed there and that commit's
/// whole reply. A session is recorded at its first commit in the partition; the metadata group's registry registers
/// sessions and checks their owners before a commit is routed, so the table takes any session it does not hold for one
/// committing there first. It takes any request number above the session's last, since the numbers the session
/// committed in other partitions leave gaps, answers a retry of the last with its stored reply, and refuses a number
/// below the last. Only the registry evicts: the table purges a session's entry when the close carrying its eviction is
/// delivered ([`Sessions::purge`]), so it holds at most the registry's `sessions_max` entries, and refuses a session it
/// does not hold as busy when full. A session has at most one commit decided and not yet applied.
///
/// **Durable** (7.8): the table keeps the sessions committed and purged since its entries were last drained, and
/// [`Sessions::drain_entries`] hands them out as entries of the keyspace without versions ([`crate::SessionEntry`]),
/// which the pipeline writes in the batch of the round that changed them; a pipeline opened again loads the table back
/// ([`Sessions::load`]). The round's log record holds each commit's entry too, so replay rebuilds the table.
#[derive(Debug, Clone)]
pub struct Sessions {
    table: HashMap<u128, Session>,
    capacity: usize,
    /// The sessions with a request decided and not yet applied.
    pending: Vec<u128>,
    /// The sessions committed since the entries were last drained.
    changed: Vec<u128>,
    /// The sessions purged since then.
    purged: Vec<u128>,
    /// Whether the set of sessions held changed since then.
    index_changed: bool,
}

impl Sessions {
    /// The default bound on sessions held, the registry's default `sessions_max`.
    pub const DEFAULT_CAPACITY: usize = 4_096;

    /// An empty table holding at most `capacity` sessions, at least one.
    pub fn new(capacity: usize) -> Self {
        Self {
            table: HashMap::new(),
            capacity: capacity.max(1),
            pending: Vec::new(),
            changed: Vec::new(),
            purged: Vec::new(),
            index_changed: false,
        }
    }

    /// The table holding at most `capacity` sessions, at least one, or the sessions it holds when more: none is evicted
    /// here, and a session it does not hold is refused while it holds `capacity` or more.
    pub fn bounded(self, capacity: usize) -> Self {
        Self {
            capacity: capacity.max(self.table.len()).max(1),
            ..self
        }
    }

    /// What a commit of `session` numbered `request` is: `Ok(None)` a new request to decide, `Ok(Some(reply))` a retry of
    /// the session's last committed request in the partition, answered with its stored reply and not committed again.
    /// Session 0 is no session and is always new.
    ///
    /// The checks run in this order: in flight, retry, request number, room. The retry before a number out of order is
    /// the Lean model's order (`lean/RoundModel/Round.lean`, `decideTxn`).
    ///
    /// # Errors
    ///
    /// [`CommitError::session_busy`] while a request of the session is decided and not yet applied;
    /// [`CommitError::request_out_of_order`] naming the last for a number below it; [`CommitError::busy`] naming the
    /// capacity for a session the full table does not hold.
    pub fn check(&self, session: u128, request: u64) -> Result<Option<&StoredReply>, CommitError> {
        if session == 0 {
            return Ok(None);
        }
        if self.pending.contains(&session) {
            return Err(CommitError::session_busy(session));
        }
        match self.table.get(&session) {
            Some(entry) if request == entry.last_request => Ok(Some(&entry.reply)),
            Some(entry) if request < entry.last_request => {
                Err(CommitError::request_out_of_order(entry.last_request))
            }
            Some(_) => Ok(None),
            None if self.table.len() >= self.capacity => Err(CommitError::busy(self.capacity)),
            None => Ok(None),
        }
    }

    /// A retry of `request` of `session` with no transaction open for it, as the frontend asks every partition after a
    /// reconnect: the stored reply when it is the session's last committed request here.
    ///
    /// # Errors
    ///
    /// [`CommitError::session_busy`] while a request of the session is decided and not yet applied;
    /// [`CommitError::request_out_of_order`] naming the last for a number below it; [`CommitError::not_committed`]
    /// otherwise: the partition holds neither the request nor a later one.
    pub fn retry(&self, session: u128, request: u64) -> Result<&StoredReply, CommitError> {
        if self.pending.contains(&session) {
            return Err(CommitError::session_busy(session));
        }
        match self.table.get(&session) {
            Some(entry) if request == entry.last_request => Ok(&entry.reply),
            Some(entry) if request < entry.last_request => {
                Err(CommitError::request_out_of_order(entry.last_request))
            }
            Some(_) | None => Err(CommitError::not_committed(request)),
        }
    }

    /// Marks a request of `session` decided and not yet applied.
    pub fn begin(&mut self, session: u128) {
        if session != 0 && !self.pending.contains(&session) {
            self.pending.push(session);
        }
    }

    /// Records `request` of `session` committed with `reply`, applied: the entry, created at the session's first commit
    /// in the partition, and no request of it in flight.
    pub fn committed(&mut self, session: u128, request: u64, reply: StoredReply) {
        self.pending.retain(|s| *s != session);
        let previous = self.table.insert(
            session,
            Session {
                last_request: request,
                reply,
            },
        );
        self.index_changed |= previous.is_none();
        self.purged.retain(|s| *s != session);
        self.changed.push(session);
        debug_assert!(self.table.contains_key(&session));
    }

    /// Purges the entries of the sessions in `evicted`, the registry's evictions a delivered close carries; a session
    /// not held is skipped.
    pub fn purge(&mut self, evicted: &[u128]) {
        for session in evicted {
            if self.table.remove(session).is_some() {
                self.changed.retain(|s| s != session);
                self.purged.push(*session);
                self.index_changed = true;
            }
        }
    }

    /// The last committed request of `session` and its version, `None` for a session not held.
    pub fn last(&self, session: u128) -> Option<(u64, u64)> {
        self.table
            .get(&session)
            .map(|e| (e.last_request, e.reply.version()))
    }

    /// The number of sessions held.
    pub fn len(&self) -> usize {
        self.table.len()
    }

    /// Whether no session is held.
    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    /// The most sessions held.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// The bytes the entries account for, 64 each and 16 for each result.
    pub fn bytes(&self) -> usize {
        self.table
            .values()
            .map(|e| ENTRY_BYTES + RESULT_BYTES * e.reply.results().len())
            .sum()
    }
}
