/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::StoreError;

use crate::traits::store_snapshot::StoreSnapshot;
use crate::types::entry::Entry;
use crate::types::item::Item;
use crate::types::persist_mode::PersistMode;
use crate::types::rebuild_record::RebuildRecord;
use crate::types::temporary::Temporary;
use crate::types::write_batch::WriteBatch;

/// The versioned store beneath the port: the managed-mode Badger surface Dgraph uses, synchronous and shaped in
/// batches. Every operation takes `&self`; an implementation serialises its writes itself.
///
/// **Versions.** A key keeps every version written to it. A read at `t` sees the newest version at or below `t`; a
/// deletion ([`Entry::deletion`]) hides its key's older versions for reads at and above its version. A version may
/// be written below the key's newest one, and rewriting a key's version replaces it. The caller never writes a key
/// below a deletion of that key: once [`Store::sweep_deletions`] has removed the deletion, such a write would be the
/// newest version for reads above it.
///
/// **Durability.** A batch handed to [`Store::apply`] survives a crash of the process; it survives a power loss
/// once a sync has covered it, by [`PersistMode::Sync`] or [`Store::sync`]. After a power loss the surviving batches
/// are a prefix of batch order, each whole. A failed write poisons the store: every later write returns
/// [`StoreError::Poisoned`] until the store is reopened, and after reopening the failed batch is present whole or
/// absent whole. Drops, bans, the discard timestamp and the tombstone horizon are durable when their call returns.
///
/// **Garbage collection.** Below the discard timestamp `D`, compaction removes every version of a key older than
/// its newest discard-flagged version or deletion at or below `D`, and never a deletion; a read below `D` is refused
/// with [`StoreError::TsTooOld`]. The sweep removes a deletion at or below the tombstone horizon that has no version
/// of its key below it.
///
/// **The keyspace without versions.** Beside the versioned keys, a store holds non-empty keys with one value each,
/// written by a batch's versionless writes ([`WriteBatch::put_versionless`]) atomically with its entries, so a batch
/// is visible whole or not at all across both. A read of it ([`Store::get_versionless`],
/// [`StoreSnapshot::get_versionless`]) is one point lookup, bounded by no discard timestamp. [`Store::drop_all`]
/// clears it; prefix drops, bans, garbage collection and temporary keyspaces leave it as it is.
///
/// **Temporary keyspaces and rebuild records.** A rebuild builds into a temporary keyspace
/// ([`Store::create_temporary`]), beside the data and outside its versions, bans, garbage collection and applied
/// timestamp, so its memory stays bounded whatever the predicate's size; opening the store drops every temporary
/// keyspace a crash left. Before it builds, it writes a rebuild record ([`Store::write_rebuild_record`]), which its
/// swap batch or failure batch removes ([`WriteBatch::remove_rebuild_record`]); the records a store holds when it is
/// opened ([`Store::rebuild_records`]) name the rebuilds a crash or a power loss stopped. A store that keeps neither
/// refuses them with [`StoreError::Unsupported`], the default.
pub trait Store {
    /// A consistent view of the store at one read timestamp.
    type Snapshot: StoreSnapshot;

    /// Writes `batch` atomically: it becomes visible whole or not at all, to point reads and scans alike, and has no
    /// size limit that splits it. A batch whose user key exceeds [`crate::MAX_USER_KEY_SIZE`] or whose stored key
    /// exceeds [`crate::MAX_STORED_KEY_SIZE`] is refused with [`StoreError::KeyTooLarge`] and writes nothing, as is a
    /// batch with an empty versionless key, refused with [`StoreError::EmptyKey`] ([`WriteBatch::check_key_sizes`]).
    /// An entry addressed to a banned namespace is dropped and the rest is written. The batch's applied timestamp, if
    /// it has one, is written in the same batch, as are its versionless writes, which no ban drops.
    fn apply(&self, batch: WriteBatch, mode: PersistMode) -> Result<(), StoreError>;

    /// Opens a snapshot reading at `read_ts`, refused with [`StoreError::TsTooOld`] below the discard timestamp.
    /// While it is open, the discard timestamp stays at or below `read_ts`.
    fn snapshot(&self, read_ts: u64) -> Result<Self::Snapshot, StoreError>;

    /// The value of `key` in the keyspace without versions, as the latest batch left it; `None` when it has none.
    fn get_versionless(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError>;

    /// The newest version of `key` at or below `read_ts`, through a snapshot of its own.
    fn get_at(&self, key: &[u8], read_ts: u64) -> Result<Option<Item>, StoreError> {
        self.snapshot(read_ts)?.get(key)
    }

    /// Removes every stored version of every key under `prefixes`, physically, then writes `entries`, such as a
    /// namespace's initial schema at timestamp 1. Commits wait while it runs; a drop interrupted by a crash or a
    /// power loss after its record was synced is finished when the store is opened again. `ts` is the drop's
    /// timestamp, kept in its record.
    fn drop_prefixes(
        &self,
        ts: u64,
        prefixes: &[Vec<u8>],
        entries: Vec<Entry>,
    ) -> Result<(), StoreError>;

    /// Removes every key, then writes `entries`; clears the banned namespaces, the discard timestamp, the tombstone
    /// horizon and the applied timestamp. Finished at open when interrupted, as [`Store::drop_prefixes`] is.
    fn drop_all(&self, entries: Vec<Entry>) -> Result<(), StoreError>;

    /// Bans `namespace`: later batches drop their entries addressed to it, and snapshots opened after the ban do
    /// not see its keys. Keys belong to the namespace [`crate::key_namespace`] reads from them.
    fn ban_namespace(&self, namespace: u64) -> Result<(), StoreError>;

    /// Asks for discard timestamp `ts`. The discard timestamp never decreases and never exceeds the oldest open
    /// snapshot's read timestamp: a request above an open snapshot takes effect as the snapshots below it close.
    fn set_discard_ts(&self, ts: u64) -> Result<(), StoreError>;

    /// The discard timestamp in effect.
    fn discard_ts(&self) -> u64;

    /// Sets the tombstone horizon, below which [`Store::sweep_deletions`] removes deletions. In effect it never
    /// exceeds the discard timestamp, and until it is set it equals the discard timestamp.
    fn set_tombstone_horizon(&self, ts: u64) -> Result<(), StoreError>;

    /// The tombstone horizon in effect.
    fn tombstone_horizon(&self) -> u64;

    /// The highest applied timestamp a batch has written, 0 after [`Store::drop_all`].
    fn applied_ts(&self) -> u64;

    /// Flushes what is in memory and compacts everything, collecting the versions below the discard timestamp.
    fn compact(&self) -> Result<(), StoreError>;

    /// At one snapshot, removes every deletion at or below the tombstone horizon that has no version of its key
    /// below it.
    fn sweep_deletions(&self) -> Result<(), StoreError>;

    /// Makes every batch written so far survive a power loss.
    fn sync(&self) -> Result<(), StoreError>;

    /// Creates an empty temporary keyspace. It survives no reopening of the store, and a write to it needs no sync.
    fn create_temporary(&self) -> Result<Temporary, StoreError> {
        Err(StoreError::Unsupported("temporary keyspaces"))
    }

    /// Writes `batch` to `temporary` atomically, as [`Store::apply`] writes to the data: refused whole when any key
    /// it holds, its versionless writes' included, breaks the limits [`WriteBatch::check_key_sizes`] checks, though
    /// the versionless writes themselves are ignored, as are the applied timestamp and the removed rebuild records; no
    /// entry is dropped for a banned namespace. A failed write poisons the store as a failed batch does.
    fn apply_temporary(&self, temporary: &Temporary, batch: WriteBatch) -> Result<(), StoreError> {
        let _ = (temporary, batch);
        Err(StoreError::Unsupported("temporary keyspaces"))
    }

    /// A snapshot of `temporary` reading every version it holds, as a snapshot of the data at `u64::MAX` reads; it
    /// holds no discard timestamp back.
    fn temporary_snapshot(&self, temporary: &Temporary) -> Result<Self::Snapshot, StoreError> {
        let _ = temporary;
        Err(StoreError::Unsupported("temporary keyspaces"))
    }

    /// Drops `temporary` and everything it holds.
    fn drop_temporary(&self, temporary: Temporary) -> Result<(), StoreError> {
        let _ = temporary;
        Err(StoreError::Unsupported("temporary keyspaces"))
    }

    /// Writes `record` and syncs it, so it survives a power loss once this returns; a record of the same number is
    /// replaced. A batch removes it ([`WriteBatch::remove_rebuild_record`]), and dropping everything removes every
    /// record.
    fn write_rebuild_record(&self, record: &RebuildRecord) -> Result<(), StoreError> {
        let _ = record;
        Err(StoreError::Unsupported("rebuild records"))
    }

    /// The rebuild records the store holds, by number: none for a store that keeps none.
    fn rebuild_records(&self) -> Result<Vec<RebuildRecord>, StoreError> {
        Ok(Vec::new())
    }
}
