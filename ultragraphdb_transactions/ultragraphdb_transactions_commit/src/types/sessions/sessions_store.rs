/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::HashMap;

use ultragraphdb_error_store::StoreError;
use ultragraphdb_traits_store::{Store, VersionlessEntry};

use crate::types::session_entry::SessionEntry;
use crate::types::sessions::{Session, Sessions};

impl Sessions {
    /// The table `store` holds, as the entries drained into the rounds it applied left it, holding at most the
    /// default capacity or the sessions it holds, whichever is more; [`Sessions::bounded`] bounds it. A session
    /// committed is touched at its commit's version, one only registered at its own number, as before.
    ///
    /// # Errors
    ///
    /// The store's error when an entry cannot be read; [`StoreError::Corrupted`] for an index whose length is not a
    /// multiple of 8 or whose numbers do not ascend from above 0, a session it names with no entry, a malformed
    /// entry, or one committed at or before its registration.
    pub fn load<S: Store>(store: &S) -> Result<Self, StoreError> {
        let Some(index) = store.get_versionless(SessionEntry::INDEX_KEY)? else {
            return Ok(Self::new(Self::DEFAULT_CAPACITY));
        };
        let corrupted =
            |key: &[u8], reason: &str| StoreError::Corrupted(key.to_vec(), reason.to_string());
        if index.len() % 8 != 0 {
            return Err(corrupted(
                SessionEntry::INDEX_KEY,
                "a session index whose length is not a multiple of 8",
            ));
        }
        let numbers: Vec<u64> = index
            .chunks_exact(8)
            .map(|n| u64::from_le_bytes(n.try_into().unwrap_or_default()))
            .collect();
        if numbers.first() == Some(&0) || numbers.windows(2).any(|w| w[0] >= w[1]) {
            return Err(corrupted(
                SessionEntry::INDEX_KEY,
                "a session index whose numbers are not ascending from above 0",
            ));
        }
        let mut table = HashMap::with_capacity(numbers.len());
        for session in numbers {
            let key = SessionEntry::key(session);
            let Some(bytes) = store.get_versionless(&key)? else {
                return Err(corrupted(&key, "the index names a session with no entry"));
            };
            let entry = SessionEntry::decode(&key, &bytes)?;
            if entry.request() != 0 && entry.version() <= session {
                return Err(corrupted(
                    &key,
                    "a session committed at or before its registration",
                ));
            }
            let session_entry = if entry.request() == 0 {
                Session::registered(session)
            } else {
                Session {
                    last_request: entry.request(),
                    last_version: entry.version(),
                    touched: entry.version(),
                    results: entry.into_results(),
                    pending: false,
                }
            };
            table.insert(session, session_entry);
        }
        let capacity = table.len().max(Self::DEFAULT_CAPACITY);
        Ok(Self {
            table,
            capacity,
            changed: Vec::new(),
            evicted: Vec::new(),
            index_changed: false,
        })
    }

    /// Drains the changes since the last drain into `out` as entries of the keyspace without versions, for the batch
    /// of the round that made them: each session registered or committed and still held, its entry, in order of
    /// number; each session evicted, the removal of its entry; and the index, when the sessions held changed.
    pub fn drain_entries(&mut self, out: &mut Vec<VersionlessEntry>) {
        self.changed.sort_unstable();
        self.changed.dedup();
        for session in self.changed.drain(..) {
            if let Some(entry) = self.table.get(&session) {
                let value = SessionEntry::new(
                    entry.last_request,
                    entry.last_version,
                    entry.results.clone(),
                );
                out.push(VersionlessEntry::put(
                    SessionEntry::key(session),
                    value.encode(),
                ));
            }
        }
        self.evicted.sort_unstable();
        for session in self.evicted.drain(..) {
            out.push(VersionlessEntry::removal(SessionEntry::key(session)));
        }
        if std::mem::take(&mut self.index_changed) {
            let mut numbers: Vec<u64> = self.table.keys().copied().collect();
            numbers.sort_unstable();
            let index = numbers.iter().flat_map(|n| n.to_le_bytes()).collect();
            out.push(VersionlessEntry::put(
                SessionEntry::INDEX_KEY.to_vec(),
                index,
            ));
        }
    }
}
