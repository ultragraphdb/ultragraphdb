/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use fjall::{Readable, Snapshot};
use ultragraphdb_error_store::StoreError;

use crate::types::drop_record::DropRecord;
use crate::types::fjall_store::Shared;
use crate::utils::lock::lock;

/// The metadata keys: the highest applied timestamp, the discard timestamp asked for, the tombstone horizon set, a
/// drop in progress, one key per banned namespace, the namespace in big-endian after the prefix, and one per rebuild
/// record.
pub const APPLIED_KEY: &[u8] = b"applied_ts";
pub const DISCARD_KEY: &[u8] = b"discard_ts";
pub const HORIZON_KEY: &[u8] = b"tombstone_horizon";
pub const DROP_KEY: &[u8] = b"drop";
pub const BANNED_PREFIX: &[u8] = b"banned/";
/// The prefix of the rebuild records, each keyed by its number in big-endian after it.
pub const REBUILD_PREFIX: &[u8] = b"rebuild/";

/// The metadata key of rebuild record `id`.
pub fn rebuild_key(id: u64) -> Vec<u8> {
    let mut key = REBUILD_PREFIX.to_vec();
    key.extend_from_slice(&id.to_be_bytes());
    key
}

/// The metadata key of banned namespace `namespace`.
pub fn banned_key(namespace: u64) -> Vec<u8> {
    let mut key = BANNED_PREFIX.to_vec();
    key.extend_from_slice(&namespace.to_be_bytes());
    key
}

impl Shared {
    /// Reads the metadata into memory and gives the compaction filter its discard timestamp.
    pub fn load_meta(&self) -> Result<(), StoreError> {
        let snapshot = self.db.snapshot();
        let applied = self.read_u64(&snapshot, APPLIED_KEY)?.unwrap_or(0);
        let discard = self.read_u64(&snapshot, DISCARD_KEY)?.unwrap_or(0);
        let horizon = self.read_u64(&snapshot, HORIZON_KEY)?;
        let banned = snapshot
            .prefix(&self.meta, BANNED_PREFIX)
            .map(|guard| {
                let key = guard
                    .key()
                    .map_err(|e| StoreError::ReadFailed(BANNED_PREFIX.to_vec(), e.to_string()))?;
                let ns: [u8; 8] = key[BANNED_PREFIX.len()..].try_into().map_err(|_| {
                    StoreError::Corrupted(key.to_vec(), "a banned namespace is eight bytes".into())
                })?;
                Ok(u64::from_be_bytes(ns))
            })
            .collect::<Result<BTreeSet<u64>, StoreError>>()?;
        self.applied_ts.store(applied, Ordering::Release);
        let mut view = lock(&self.view);
        view.discard_ts = discard;
        view.horizon = horizon;
        view.banned = Arc::new(banned);
        self.publish_discard(&view);
        Ok(())
    }

    /// The drop in progress, if a crash or power loss interrupted one.
    pub fn drop_record(&self) -> Result<Option<DropRecord>, StoreError> {
        let bytes = self
            .db
            .snapshot()
            .get(&self.meta, DROP_KEY)
            .map_err(|e| StoreError::ReadFailed(DROP_KEY.to_vec(), e.to_string()))?;
        bytes
            .map(|b| {
                DropRecord::decode(&b)
                    .map_err(|reason| StoreError::Corrupted(DROP_KEY.to_vec(), reason))
            })
            .transpose()
    }

    fn read_u64(&self, snapshot: &Snapshot, key: &[u8]) -> Result<Option<u64>, StoreError> {
        let bytes = snapshot
            .get(&self.meta, key)
            .map_err(|e| StoreError::ReadFailed(key.to_vec(), e.to_string()))?;
        bytes
            .map(|b| {
                let raw: [u8; 8] = (*b).try_into().map_err(|_| {
                    StoreError::Corrupted(
                        key.to_vec(),
                        format!("{} bytes where eight belong", b.len()),
                    )
                })?;
                Ok(u64::from_be_bytes(raw))
            })
            .transpose()
    }
}
