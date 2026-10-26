/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;
use std::sync::atomic::Ordering;

use fjall::{Keyspace, KeyspaceCreateOptions};
use ultragraphdb_error_store::StoreError;
use ultragraphdb_traits_store::{Temporary, WriteBatch};

use crate::types::fjall_snapshot::FjallSnapshot;
use crate::types::fjall_store::{Shared, TEMPORARY_KEYSPACE_PREFIX};
use crate::utils::lock::lock;
use crate::utils::stored_key::stored_key;
use crate::utils::stored_value::encode_value;

impl Shared {
    /// Drops every temporary keyspace a process left: none survives an opening.
    pub fn drop_left_temporaries(&self) -> Result<(), StoreError> {
        let left: Vec<String> = self
            .db
            .list_keyspace_names()
            .iter()
            .filter(|name| name.starts_with(TEMPORARY_KEYSPACE_PREFIX))
            .map(|name| name.to_string())
            .collect();
        for name in &left {
            let opened = |e: fjall::Error| StoreError::OpenFailed(e.to_string());
            let keyspace = self
                .db
                .keyspace(name, KeyspaceCreateOptions::default)
                .map_err(opened)?;
            self.db.delete_keyspace(keyspace).map_err(opened)?;
        }
        if !left.is_empty() {
            tracing::warn!(
                keyspaces = left.len(),
                "dropped the temporary keyspaces a crash left"
            );
        }
        Ok(())
    }

    pub fn create_temporary(&self) -> Result<Temporary, StoreError> {
        self.refuse_if_poisoned()?;
        let id = self.next_temporary.fetch_add(1, Ordering::AcqRel);
        let keyspace = self
            .db
            .keyspace(
                &format!("{TEMPORARY_KEYSPACE_PREFIX}{id}"),
                KeyspaceCreateOptions::default,
            )
            .map_err(|e| self.failed(e))?;
        lock(&self.temporaries).insert(id, keyspace);
        Ok(Temporary::new(id))
    }

    fn temporary(&self, temporary: &Temporary) -> Result<Keyspace, StoreError> {
        lock(&self.temporaries)
            .get(&temporary.id())
            .cloned()
            .ok_or_else(|| StoreError::UnknownTemporary(temporary.id()))
    }

    /// One fjall batch to the temporary keyspace, persisted no further than the journal's buffer: a temporary
    /// keyspace survives no reopening, so nothing it holds needs a sync. The batch is refused whole when any key it
    /// holds, its versionless writes' included, breaks the limits `WriteBatch::check_key_sizes` checks, though the
    /// versionless writes themselves are ignored.
    pub fn apply_temporary(
        &self,
        temporary: &Temporary,
        batch: WriteBatch,
    ) -> Result<(), StoreError> {
        self.refuse_if_poisoned()?;
        let keyspace = self.temporary(temporary)?;
        batch.check_key_sizes()?;
        let mut fb = self.db.batch();
        for e in batch.into_entries() {
            fb.insert(
                &keyspace,
                stored_key(e.key(), e.version()),
                encode_value(&e, false),
            );
        }
        self.commit(fb)
    }

    pub fn temporary_snapshot(
        self: &Arc<Self>,
        temporary: &Temporary,
    ) -> Result<FjallSnapshot, StoreError> {
        let keyspace = self.temporary(temporary)?;
        Ok(FjallSnapshot::temporary(
            self.clone(),
            self.db.snapshot(),
            keyspace,
        ))
    }

    pub fn drop_temporary(&self, temporary: Temporary) -> Result<(), StoreError> {
        self.refuse_if_poisoned()?;
        let keyspace = lock(&self.temporaries)
            .remove(&temporary.id())
            .ok_or(StoreError::UnknownTemporary(temporary.id()))?;
        self.db
            .delete_keyspace(keyspace)
            .map_err(|e| self.failed(e))
    }
}
