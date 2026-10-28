/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod sessions_store;

use std::collections::HashMap;

use ultragraphdb_protocol_wire::OperationResult;

use crate::errors::commit_error::CommitError;

/// One session's entry: its last committed request and that commit's whole reply, its version and its operations'
/// results; the version it was last registered or committed at, which orders eviction; and whether a request of it is
/// decided and not yet applied.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Session {
    last_request: u64,
    last_version: u64,
    results: Vec<OperationResult>,
    touched: u64,
    pending: bool,
}

/// The bytes one entry accounts for: its key and its four fields, and 16 for each result.
const ENTRY_BYTES: usize = 40;
const RESULT_BYTES: usize = 16;

/// The session table (D9, after TigerBeetle's client sessions): each registered session's last committed request and
/// its whole reply, bounded, the oldest session evicted. A session is numbered by the commit version its registration
/// took, so numbers are unique and rise. Request numbers committed are kept apart from those decided and not yet
/// applied (`pending`), so a session has at most one commit in flight. Keyed by a number a client sends, so the map is
/// std's, with its seeded hasher; it allocates when a session is registered, never when one commits.
///
/// **Durable** (7.8): the table keeps the sessions registered, committed and evicted since its entries were last
/// drained, and [`Sessions::drain_entries`] hands them out as entries of the keyspace without versions
/// ([`crate::SessionEntry`]), which the pipeline writes in the batch of the round that changed them; a pipeline opened
/// again loads the table back ([`Sessions::load`]).
#[derive(Debug, Clone)]
pub struct Sessions {
    table: HashMap<u64, Session>,
    capacity: usize,
    /// The sessions registered or committed since the entries were last drained.
    changed: Vec<u64>,
    /// The sessions evicted since then.
    evicted: Vec<u64>,
    /// Whether the set of sessions held changed since then.
    index_changed: bool,
}

impl Sessions {
    /// The default bound on sessions held.
    pub const DEFAULT_CAPACITY: usize = 4_096;

    /// An empty table holding at most `capacity` sessions, at least one.
    pub fn new(capacity: usize) -> Self {
        Self {
            table: HashMap::new(),
            capacity: capacity.max(1),
            changed: Vec::new(),
            evicted: Vec::new(),
            index_changed: false,
        }
    }

    /// The table holding at most `capacity` sessions, at least one, the oldest evicted until it does.
    pub fn bounded(mut self, capacity: usize) -> Self {
        self.capacity = capacity.max(1);
        while self.table.len() > self.capacity {
            self.evict_oldest();
        }
        self
    }

    /// What a commit of `session` numbered `request` is, in the order the Lean model checks it: `Ok(None)` a new
    /// request to decide, `Ok(Some((version, results)))` a retry of the session's last committed request, answered
    /// with its stored reply and not committed again. Session 0 is no session and is always new.
    ///
    /// # Errors
    ///
    /// [`CommitError::SessionEvicted`] for a session the table does not hold; [`CommitError::SessionBusy`] while a
    /// request of it is decided and not yet applied; [`CommitError::RequestOutOfOrder`] for a number neither the last
    /// nor one above it.
    pub fn check(
        &self,
        session: u64,
        request: u64,
    ) -> Result<Option<(u64, &[OperationResult])>, CommitError> {
        if session == 0 {
            return Ok(None);
        }
        let Some(entry) = self.table.get(&session) else {
            return Err(CommitError::SessionEvicted(session));
        };
        if entry.pending {
            return Err(CommitError::SessionBusy(session));
        }
        if request == entry.last_request && entry.last_request != 0 {
            return Ok(Some((entry.last_version, &entry.results)));
        }
        let expected = entry.last_request.saturating_add(1);
        if request != expected {
            return Err(CommitError::RequestOutOfOrder(expected));
        }
        Ok(None)
    }

    /// Marks a request of `session` decided and not yet applied.
    pub fn begin(&mut self, session: u64) {
        if let Some(entry) = self.table.get_mut(&session) {
            entry.pending = true;
        }
    }

    /// Records `request` of `session` committed at `version` with its operations' `results`, applied: the stored
    /// reply.
    pub fn committed(
        &mut self,
        session: u64,
        request: u64,
        version: u64,
        results: Vec<OperationResult>,
    ) {
        if let Some(entry) = self.table.get_mut(&session) {
            *entry = Session {
                last_request: request,
                last_version: version,
                results,
                touched: version,
                pending: false,
            };
            self.changed.push(session);
        }
    }

    /// Registers session `session`, numbered by its registration's version, evicting the session touched longest
    /// ago when the table is full, one with no request in flight first: the session evicted.
    pub fn register(&mut self, session: u64) -> Option<u64> {
        let evicted = if self.table.len() >= self.capacity {
            self.evict_oldest()
        } else {
            None
        };
        self.table.insert(session, Session::registered(session));
        self.changed.push(session);
        self.index_changed = true;
        evicted
    }

    /// Evicts the session touched longest ago, one with no request in flight first: the session evicted.
    fn evict_oldest(&mut self) -> Option<u64> {
        let oldest = self
            .table
            .iter()
            .min_by_key(|(number, entry)| (entry.pending, entry.touched, **number))
            .map(|(number, _)| *number)?;
        self.table.remove(&oldest);
        self.evicted.push(oldest);
        self.index_changed = true;
        Some(oldest)
    }

    /// The last committed request of `session` and its version, `None` for a session not held.
    pub fn last(&self, session: u64) -> Option<(u64, u64)> {
        self.table
            .get(&session)
            .map(|e| (e.last_request, e.last_version))
    }

    /// The number of sessions held.
    pub fn len(&self) -> usize {
        self.table.len()
    }

    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    /// The most sessions held.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// The bytes the entries account for, 40 each and 16 for each result.
    pub fn bytes(&self) -> usize {
        self.table
            .values()
            .map(|e| ENTRY_BYTES + RESULT_BYTES * e.results.len())
            .sum()
    }
}

impl Session {
    /// The entry of a session registered at version `session` and not yet committed.
    fn registered(session: u64) -> Self {
        Self {
            last_request: 0,
            last_version: 0,
            results: Vec::new(),
            touched: session,
            pending: false,
        }
    }
}
