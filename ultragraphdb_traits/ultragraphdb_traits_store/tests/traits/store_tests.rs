/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::Cell;

use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_traits_store::{
    Entry, Item, PersistMode, RebuildRecord, Scan, Store, StoreSnapshot, Temporary, WriteBatch,
};

/// A store holding one item, at version 3, and refusing reads below 2.
#[derive(Default)]
struct OneItem {
    opened_at: Cell<Option<u64>>,
}

struct OneItemSnapshot(u64);

impl StoreSnapshot for OneItemSnapshot {
    type Scan = std::vec::IntoIter<Result<Item, StoreError>>;

    fn read_ts(&self) -> u64 {
        self.0
    }

    fn get(&self, key: &[u8]) -> Result<Option<Item>, StoreError> {
        Ok((key == b"k" && self.0 >= 3)
            .then(|| Item::new(key.to_vec(), 3, 0x08, false, b"v".to_vec())))
    }

    fn get_many<K: AsRef<[u8]>>(&self, keys: &[K]) -> Result<Vec<Option<Item>>, StoreError> {
        keys.iter().map(|k| self.get(k.as_ref())).collect()
    }

    fn scan(&self, _: &Scan) -> Self::Scan {
        Vec::new().into_iter()
    }
}

impl Store for OneItem {
    type Snapshot = OneItemSnapshot;

    fn apply(&self, _: WriteBatch, _: PersistMode) -> Result<(), StoreError> {
        Ok(())
    }

    fn snapshot(&self, read_ts: u64) -> Result<Self::Snapshot, StoreError> {
        self.opened_at.set(Some(read_ts));
        if read_ts < 2 {
            return Err(StoreError::TsTooOld());
        }
        Ok(OneItemSnapshot(read_ts))
    }

    fn drop_prefixes(&self, _: u64, _: &[Vec<u8>], _: Vec<Entry>) -> Result<(), StoreError> {
        Ok(())
    }

    fn drop_all(&self, _: Vec<Entry>) -> Result<(), StoreError> {
        Ok(())
    }

    fn ban_namespace(&self, _: u64) -> Result<(), StoreError> {
        Ok(())
    }

    fn set_discard_ts(&self, _: u64) -> Result<(), StoreError> {
        Ok(())
    }

    fn discard_ts(&self) -> u64 {
        2
    }

    fn set_tombstone_horizon(&self, _: u64) -> Result<(), StoreError> {
        Ok(())
    }

    fn tombstone_horizon(&self) -> u64 {
        2
    }

    fn applied_ts(&self) -> u64 {
        3
    }

    fn compact(&self) -> Result<(), StoreError> {
        Ok(())
    }

    fn sweep_deletions(&self) -> Result<(), StoreError> {
        Ok(())
    }

    fn sync(&self) -> Result<(), StoreError> {
        Ok(())
    }
}

#[test]
fn test_get_at_reads_through_a_snapshot_at_its_timestamp() {
    let store = OneItem::default();
    let item = store.get_at(b"k", 5).unwrap().expect("found at 5");
    assert_eq!((item.version(), item.value()), (3, &b"v"[..]));
    assert_eq!(store.opened_at.get(), Some(5));
    assert_eq!(store.get_at(b"k", 2).unwrap(), None);
}

#[test]
fn test_get_at_passes_on_the_snapshots_refusal() {
    let store = OneItem::default();
    assert_eq!(store.get_at(b"k", 1), Err(StoreError::TsTooOld()));
    assert_eq!(store.opened_at.get(), Some(1));
}

/// A store that keeps no temporary keyspaces and no rebuild records refuses them by name, and holds no record.
#[test]
fn test_a_store_without_temporary_keyspaces_or_records_refuses_them() {
    let store = OneItem::default();
    let unsupported = |e: StoreError| matches!(e.0, StoreErrorEnum::Unsupported(_));
    assert!(store.create_temporary().is_err_and(unsupported));
    let t = Temporary::new(1);
    assert!(
        store
            .apply_temporary(&t, WriteBatch::new())
            .is_err_and(unsupported)
    );
    assert!(store.temporary_snapshot(&t).is_err_and(unsupported));
    assert!(store.drop_temporary(t).is_err_and(unsupported));
    assert!(
        store
            .write_rebuild_record(&RebuildRecord::new(1, Vec::new()))
            .is_err_and(unsupported)
    );
    assert_eq!(store.rebuild_records(), Ok(Vec::new()));
}
