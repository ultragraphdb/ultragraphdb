/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use fjall::{Readable, Snapshot};
use ultragraphdb_error_store::StoreError;

use crate::types::drop_record::DropRecord;
use crate::types::fjall_store::Shared;
use crate::utils::lock::lock;

/// The metadata keys: the highest applied timestamp, the discard timestamp asked for, the tombstone horizon set, a
/// drop in progress, one key per banned tenant, the tenant in big-endian after the prefix, and one per rebuild
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

/// The metadata key of banned tenant `tenant`.
pub fn banned_key(tenant: u64) -> Vec<u8> {
    let mut key = BANNED_PREFIX.to_vec();
    key.extend_from_slice(&tenant.to_be_bytes());
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
                    .map_err(|e| StoreError::read_failed(BANNED_PREFIX.to_vec(), e.to_string()))?;
                let tenant: [u8; 8] = key
                    .get(BANNED_PREFIX.len()..)
                    .and_then(|tenant| tenant.try_into().ok())
                    .ok_or_else(|| {
                        StoreError::corrupted(key.to_vec(), "a banned tenant is eight bytes".into())
                    })?;
                Ok(u64::from_be_bytes(tenant))
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

    /// Writes the discard timestamp `ts` asks for, the one held if that is higher, under the writer lock and into the journal file,
    /// synced by nobody yet: its place in the journal keeps it behind every batch it may discard from. What it wrote.
    ///
    /// # Errors
    ///
    /// [`StoreError::poisoned`] for a poisoned store; the failed write's error, which poisons it.
    pub fn write_discard(&self, ts: u64) -> Result<u64, StoreError> {
        let _writer = lock(&self.writer);
        self.refuse_if_poisoned()?;
        let asked = lock(&self.view).discard_ts.max(ts);
        let mut batch = self.db.batch().durability(Some(fjall::PersistMode::Buffer));
        batch.insert(&self.meta, DISCARD_KEY, asked.to_be_bytes().to_vec());
        self.commit(batch)?;
        Ok(asked)
    }

    /// The journal file with the highest number, the active one when listed. fjall names them `{number}.jnl` in the store's
    /// directory (`fjall-3.1.12/src/journal/writer.rs:110`), one more reason fjall is pinned exactly; a store with no journal is one
    /// fjall left in a layout this does not know, and syncing nothing would lie.
    ///
    /// # Errors
    ///
    /// The directory's read error, which poisons the store; [`StoreError::write_failed`] when it holds no journal.
    pub fn newest_journal(&self) -> Result<PathBuf, StoreError> {
        let failed = |error| self.failed(fjall::Error::Io(error));
        let mut newest: Option<(u64, PathBuf)> = None;
        for entry in std::fs::read_dir(&self.path).map_err(failed)? {
            let path = entry.map_err(failed)?.path();
            let number = path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_suffix(".jnl"))
                .and_then(|number| number.parse::<u64>().ok());
            if let Some(number) = number
                && newest.as_ref().is_none_or(|(held, _)| number > *held)
            {
                newest = Some((number, path));
            }
        }
        newest.map(|(_, path)| path).ok_or_else(|| {
            StoreError::write_failed(format!("no journal in {}", self.path.display()))
        })
    }

    /// Makes `journal`, listed after a record was written, durable, through a handle of its own: `Database::persist` holds fjall's
    /// journal lock for its whole fsync (`fjall-3.1.12/src/journal/mod.rs:122`), which every commit takes, so that way a commit waits
    /// for the sync and a partition's core with it. Fjall syncs a journal as it seals it, and only a sealed one is removed, so a
    /// journal found gone holds nothing unsynced, and every journal older than `journal` was synced the same way.
    ///
    /// # Errors
    ///
    /// The open's or the sync's error, other than the journal being gone, which poisons the store.
    pub fn sync_journal(&self, journal: &Path) -> Result<(), StoreError> {
        match OpenOptions::new().append(true).open(journal) {
            Ok(file) => file
                .sync_all()
                .map_err(|error| self.failed(fjall::Error::Io(error))),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(self.failed(fjall::Error::Io(error))),
        }
    }

    /// The drop in progress, if a crash or power loss interrupted one.
    pub fn drop_record(&self) -> Result<Option<DropRecord>, StoreError> {
        let bytes = self
            .db
            .snapshot()
            .get(&self.meta, DROP_KEY)
            .map_err(|e| StoreError::read_failed(DROP_KEY.to_vec(), e.to_string()))?;
        bytes
            .map(|b| {
                DropRecord::decode(&b)
                    .map_err(|reason| StoreError::corrupted(DROP_KEY.to_vec(), reason))
            })
            .transpose()
    }

    fn read_u64(&self, snapshot: &Snapshot, key: &[u8]) -> Result<Option<u64>, StoreError> {
        let bytes = snapshot
            .get(&self.meta, key)
            .map_err(|e| StoreError::read_failed(key.to_vec(), e.to_string()))?;
        bytes
            .map(|b| {
                let raw: [u8; 8] = (*b).try_into().map_err(|_| {
                    StoreError::corrupted(
                        key.to_vec(),
                        format!("{} bytes where eight belong", b.len()),
                    )
                })?;
                Ok(u64::from_be_bytes(raw))
            })
            .transpose()
    }
}
