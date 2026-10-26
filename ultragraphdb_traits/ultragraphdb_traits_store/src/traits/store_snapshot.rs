/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::StoreError;

use crate::types::item::Item;
use crate::types::scan::Scan;

/// A consistent view of a [`crate::Store`] at one read timestamp: every read through it, point or scan, sees the
/// same batches, and a batch written after it was opened is invisible to it. Closing it, by dropping it, releases
/// its hold on the discard timestamp.
pub trait StoreSnapshot {
    /// The items of a scan, in scan order. A read that fails, at once or partway through, yields its error and
    /// then ends.
    type Scan: Iterator<Item = Result<Item, StoreError>>;

    /// The timestamp every read of this snapshot is at.
    fn read_ts(&self) -> u64;

    /// The newest version of `key` at or below the read timestamp; `None` when there is none or it is a deletion.
    fn get(&self, key: &[u8]) -> Result<Option<Item>, StoreError>;

    /// [`StoreSnapshot::get`] for each of `keys`, in their order, in one call.
    fn get_many<K: AsRef<[u8]>>(&self, keys: &[K]) -> Result<Vec<Option<Item>>, StoreError>;

    /// The keys `scan` selects, each once with its newest version at or below the read timestamp, or with each of
    /// its versions newest first when the scan asks for all versions. A key's versions end at its newest deletion
    /// at or below the read timestamp, which is not reported, and a key whose newest such version is a deletion is
    /// absent.
    fn scan(&self, scan: &Scan) -> Self::Scan;

    /// The value of `key` in the keyspace without versions, as the batches this snapshot sees left it; `None` when it
    /// has none. A snapshot of a temporary keyspace holds none.
    fn get_versionless(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError>;

    /// The versions of `key` at or below the read timestamp, newest first, down to its newest deletion.
    fn versions(&self, key: &[u8]) -> Self::Scan {
        self.scan(&Scan::key(key.to_vec()).all_versions())
    }
}
