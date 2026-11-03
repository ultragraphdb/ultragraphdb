/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::HashMap;

use ultragraphdb_error_store::StoreError;
use ultragraphdb_traits_store::{Store, VersionlessEntry};

use crate::types::session_entry::SessionEntry;
use crate::types::sessions::{Session, Sessions};

/// A session identity's bytes in the index.
const SESSION_BYTES: usize = 16;

impl Sessions {
    /// The table `store` holds, as the entries drained into the rounds it applied left it, holding at most the
    /// default capacity or the sessions it holds, whichever is more; [`Sessions::bounded`] bounds it.
    ///
    /// # Errors
    ///
    /// The store's error when an entry cannot be read; [`StoreError::corrupted`] for an index whose length is not a
    /// multiple of 16 or whose sessions are not ascending and above 0, a session it names with no entry, or a
    /// malformed entry.
    pub fn load<S: Store>(store: &S) -> Result<Self, StoreError> {
        let Some(index) = store.get_versionless(SessionEntry::INDEX_KEY)? else {
            return Ok(Self::new(Self::DEFAULT_CAPACITY));
        };
        let corrupted =
            |key: &[u8], reason: &str| StoreError::corrupted(key.to_vec(), reason.to_string());
        let (sessions, rest) = index.as_chunks::<SESSION_BYTES>();
        if !rest.is_empty() {
            return Err(corrupted(
                SessionEntry::INDEX_KEY,
                "a session index whose length is not a multiple of 16",
            ));
        }
        let sessions: Vec<u128> = sessions.iter().map(|s| u128::from_be_bytes(*s)).collect();
        if sessions.first() == Some(&0) || !sessions.is_sorted_by(|a, b| a < b) {
            return Err(corrupted(
                SessionEntry::INDEX_KEY,
                "a session index whose sessions are not ascending from above 0",
            ));
        }
        let mut table = HashMap::with_capacity(sessions.len());
        for session in sessions {
            let key = SessionEntry::key(session);
            let Some(bytes) = store.get_versionless(&key)? else {
                return Err(corrupted(&key, "the index names a session with no entry"));
            };
            let entry = SessionEntry::decode(&key, &bytes)?;
            let last_request = entry.request();
            let session_entry = Session {
                last_request,
                reply: entry.into_reply(),
            };
            let _replaced: Option<Session> = table.insert(session, session_entry);
        }
        let capacity = table.len().max(Self::DEFAULT_CAPACITY);
        Ok(Self {
            table,
            capacity,
            pending: Vec::new(),
            changed: Vec::new(),
            purged: Vec::new(),
            index_changed: false,
        })
    }

    /// Drains the changes since the last drain into `out` as entries of the keyspace without versions, for the batch
    /// of the round that made them: each session committed and still held, its entry, in order of identity; each
    /// session purged, the removal of its entry; and the index, when the sessions held changed.
    pub fn drain_entries(&mut self, out: &mut Vec<VersionlessEntry>) {
        self.changed.sort_unstable();
        self.changed.dedup();
        for session in self.changed.drain(..) {
            if let Some(entry) = self.table.get(&session) {
                let value = SessionEntry::new(entry.last_request, entry.reply.clone());
                out.push(VersionlessEntry::put(
                    SessionEntry::key(session),
                    value.encode(),
                ));
            }
        }
        self.purged.sort_unstable();
        self.purged.dedup();
        for session in self.purged.drain(..) {
            out.push(VersionlessEntry::removal(SessionEntry::key(session)));
        }
        if std::mem::take(&mut self.index_changed) {
            let mut sessions: Vec<u128> = self.table.keys().copied().collect();
            sessions.sort_unstable();
            let index = sessions.iter().flat_map(|s| s.to_be_bytes()).collect();
            out.push(VersionlessEntry::put(
                SessionEntry::INDEX_KEY.to_vec(),
                index,
            ));
        }
    }
}
