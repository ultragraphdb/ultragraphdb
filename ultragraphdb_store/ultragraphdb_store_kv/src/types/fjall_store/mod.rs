/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod fjall_store_debug;
mod fjall_store_store;
mod store_drops;
mod store_gc;
mod store_meta;
mod store_rebuild_records;
mod store_temporaries;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use fjall::compaction::filter::Factory;
use fjall::{Database, Keyspace, KeyspaceCreateOptions, OwnedWriteBatch, Readable};
use ultragraphdb_error_store::StoreError;
use ultragraphdb_traits_store::{Entry, PersistMode};

use crate::types::durability::Durability;
use crate::types::gc_filter_factory::GcFilterFactory;
use crate::types::gc_settings::GcSettings;
use crate::types::syncer::Syncer;
use crate::utils::lock::lock;
use crate::utils::stored_key::decode_stored_key;
use crate::utils::stored_value::decode_value;

/// The fjall keyspace holding every version of every key.
pub const DATA_KEYSPACE: &str = "data";
/// The fjall keyspace holding the store's metadata: the applied timestamp, the banned namespaces, the discard
/// timestamp, the tombstone horizon, a drop in progress and the rebuild records.
pub const META_KEYSPACE: &str = "meta";
/// The name of every temporary keyspace begins with this, its number after it; opening the store drops each.
pub const TEMPORARY_KEYSPACE_PREFIX: &str = "tmp-";

/// The [`ultragraphdb_traits_store::Store`] over fjall: the managed-mode Badger surface Dgraph uses, in one fjall
/// `Database` of two keyspaces, data and metadata, beside which a rebuild creates temporary keyspaces.
///
/// A version is a stored key of its own (the user key escaped and terminated, then the complement of the version),
/// so fjall's byte order is Badger's, and its value carries the meta byte, the discard flag and the tombstone flag in
/// a prefix. Every read goes through one `Database::snapshot()` covering both keyspaces. Writes are serialised by one
/// writer lock and each is one fjall batch. Garbage collection is a compaction filter under the discard timestamp,
/// and [`ultragraphdb_traits_store::Store::compact`] flushes the memtables and compacts everything into the last
/// level.
///
/// A [`crate::FjallSnapshot`] keeps the database open: drop every snapshot before reopening the directory.
pub struct FjallStore {
    // Dropped first: the sync thread is joined before the database closes.
    _syncer: Option<Syncer>,
    shared: Arc<Shared>,
}

/// What the store and its snapshots share.
pub(crate) struct Shared {
    pub db: Database,
    pub data: Keyspace,
    pub meta: Keyspace,
    pub durability: Durability,
    /// Held by every write for its whole length, so that writes apply in order and a drop blocks commits.
    pub writer: Mutex<()>,
    pub view: Mutex<View>,
    pub poisoned: AtomicBool,
    pub applied_ts: AtomicU64,
    pub gc: Arc<GcSettings>,
    /// The temporary keyspaces by number, and the next number.
    pub temporaries: Mutex<BTreeMap<u64, Keyspace>>,
    pub next_temporary: AtomicU64,
}

/// What a snapshot is opened against, changed under its lock.
#[derive(Debug, Default)]
pub(crate) struct View {
    /// The banned namespaces; a snapshot keeps the set it was opened with.
    pub banned: Arc<BTreeSet<u64>>,
    /// The read timestamps of the open snapshots, with how many are open at each.
    pub readers: BTreeMap<u64, usize>,
    /// The discard timestamp asked for, which the store keeps.
    pub discard_ts: u64,
    /// The tombstone horizon set, if one was.
    pub horizon: Option<u64>,
}

impl View {
    /// The discard timestamp in effect: the one asked for, held at or below the oldest open snapshot.
    pub fn discard_in_effect(&self) -> u64 {
        let oldest = self.readers.keys().next().copied().unwrap_or(u64::MAX);
        self.discard_ts.min(oldest)
    }

    /// The tombstone horizon in effect: the one set, or the discard timestamp, and never above it.
    pub fn horizon_in_effect(&self) -> u64 {
        let discard = self.discard_in_effect();
        self.horizon.unwrap_or(discard).min(discard)
    }
}

impl FjallStore {
    /// Opens the store in `path`, creating it when the directory holds none, finishes a drop a crash or a power loss
    /// interrupted and drops the temporary keyspaces a process left before it serves.
    pub fn open(path: impl AsRef<Path>, durability: Durability) -> Result<Self, StoreError> {
        Self::open_with_sync_period(path, durability, Syncer::PERIOD)
    }

    /// [`FjallStore::open`] with the period of [`Durability::Process`]'s sync.
    pub fn open_with_sync_period(
        path: impl AsRef<Path>,
        durability: Durability,
        period: Duration,
    ) -> Result<Self, StoreError> {
        let gc = Arc::new(GcSettings::default());
        let factory: Arc<dyn Factory> = Arc::new(GcFilterFactory::new(gc.clone()));
        let opened = |e: fjall::Error| StoreError::OpenFailed(e.to_string());
        let db = Database::builder(path.as_ref())
            .with_compaction_filter_factories(Arc::new(move |name: &str| {
                (name == DATA_KEYSPACE).then(|| factory.clone())
            }))
            .open()
            .map_err(opened)?;
        let data = db
            .keyspace(DATA_KEYSPACE, KeyspaceCreateOptions::default)
            .map_err(opened)?;
        let meta = db
            .keyspace(META_KEYSPACE, KeyspaceCreateOptions::default)
            .map_err(opened)?;
        let shared = Arc::new(Shared {
            db,
            data,
            meta,
            durability,
            writer: Mutex::new(()),
            view: Mutex::new(View::default()),
            poisoned: AtomicBool::new(false),
            applied_ts: AtomicU64::new(0),
            gc,
            temporaries: Mutex::new(BTreeMap::new()),
            next_temporary: AtomicU64::new(1),
        });
        shared.load_meta()?;
        if let Some(record) = shared.drop_record()? {
            tracing::warn!(
                ts = record.ts,
                prefixes = record.prefixes.as_ref().map(Vec::len),
                entries = record.entries.len(),
                "finishing a drop a crash or power loss interrupted"
            );
            shared.finish_drop(&record)?;
        }
        shared.drop_left_temporaries()?;
        let syncer = match durability {
            Durability::Process => Some(Syncer::start(Arc::downgrade(&shared), period)?),
            Durability::Filesystem => None,
        };
        tracing::info!(
            path = %path.as_ref().display(),
            durability = ?durability,
            applied_ts = shared.applied_ts.load(Ordering::Acquire),
            "store opened"
        );
        Ok(Self {
            _syncer: syncer,
            shared,
        })
    }

    /// Every stored version of every key, deletions included, read at one snapshot whatever its timestamp: keys in
    /// order, each key's versions newest first. A deletion is an [`Entry::deletion`].
    pub fn dump(&self) -> Result<Vec<Entry>, StoreError> {
        let snapshot = self.shared.db.snapshot();
        snapshot
            .iter(&self.shared.data)
            .map(|guard| {
                let (stored, bytes) = guard
                    .into_inner()
                    .map_err(|e| StoreError::ReadFailed(Vec::new(), e.to_string()))?;
                let (key, version) = decode_stored_key(&stored)
                    .map_err(|reason| StoreError::Corrupted(stored.to_vec(), reason))?;
                let v = decode_value(&bytes)
                    .map_err(|reason| StoreError::Corrupted(key.clone(), reason))?;
                Ok(match (v.deletion, v.discard) {
                    (true, _) => Entry::deletion(key, version),
                    (false, false) => Entry::put(key, version, v.meta, v.value.to_vec()),
                    (false, true) => {
                        Entry::put(key, version, v.meta, v.value.to_vec()).with_discard()
                    }
                })
            })
            .collect()
    }

    /// The items compactions kept because they could not decode them, since the store was opened.
    pub fn undecodable_kept(&self) -> u64 {
        self.shared.gc.undecodable()
    }

    /// Whether a failed write has poisoned the store, which then recovers only by reopening.
    pub fn is_poisoned(&self) -> bool {
        self.shared.poisoned.load(Ordering::Acquire)
    }
}

impl Shared {
    pub fn refuse_if_poisoned(&self) -> Result<(), StoreError> {
        if self.poisoned.load(Ordering::Acquire) {
            return Err(StoreError::Poisoned());
        }
        Ok(())
    }

    /// The error of a failed fjall write, which poisons the store as it poisons fjall.
    pub fn failed(&self, error: fjall::Error) -> StoreError {
        if !self.poisoned.swap(true, Ordering::AcqRel) {
            tracing::error!(error = %error, "store poisoned by a failed write; it must be reopened");
        }
        match error {
            fjall::Error::Poisoned => StoreError::Poisoned(),
            other => StoreError::WriteFailed(other.to_string()),
        }
    }

    /// A batch persisted as `mode` asks and the store's durability requires.
    pub fn batch(&self, mode: PersistMode) -> OwnedWriteBatch {
        let mode = match (mode, self.durability) {
            (PersistMode::Buffer, Durability::Process) => fjall::PersistMode::Buffer,
            _ => fjall::PersistMode::SyncAll,
        };
        self.db.batch().durability(Some(mode))
    }

    /// A metadata batch, synced: what compaction acts on is durable when the call that sets it returns.
    pub fn synced_batch(&self) -> OwnedWriteBatch {
        self.db
            .batch()
            .durability(Some(fjall::PersistMode::SyncAll))
    }

    pub fn commit(&self, batch: OwnedWriteBatch) -> Result<(), StoreError> {
        batch.commit().map_err(|e| self.failed(e))
    }

    pub fn sync(&self) -> Result<(), StoreError> {
        self.db
            .persist(fjall::PersistMode::SyncAll)
            .map_err(|e| self.failed(e))
    }

    /// The sync of [`Durability::Process`]: skipped while the store is poisoned, and poisoning it when it fails.
    pub fn periodic_sync(&self) {
        let _writer = lock(&self.writer);
        if self.refuse_if_poisoned().is_ok() {
            let _ = self.sync();
        }
    }

    /// Records the discard timestamp in effect for the compaction filter.
    pub fn publish_discard(&self, view: &View) {
        self.gc.set_discard_ts(view.discard_in_effect());
    }
}
