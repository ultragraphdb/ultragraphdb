/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::Cell;

use ultragraphdb_error_store::StoreError;
use ultragraphdb_traits_store::{
    Entry, Item, PersistMode, RebuildRecord, Scan, Store, StoreSnapshot, Temporary, WriteBatch,
};

/// A store holding one item, at version 3, refusing reads below 2 and failing every read of `bad`.
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
        if key == b"bad" {
            return Err(StoreError::ReadFailed(key.to_vec(), "checksum".to_string()));
        }
        Ok((key == b"k" && self.0 >= 3)
            .then(|| Item::new(key.to_vec(), 3, 0x08, false, b"v".to_vec())))
    }

    fn get_many<K: AsRef<[u8]>>(&self, keys: &[K]) -> Result<Vec<Option<Item>>, StoreError> {
        keys.iter().map(|k| self.get(k.as_ref())).collect()
    }

    fn scan(&self, _: &Scan) -> Self::Scan {
        Vec::new().into_iter()
    }

    fn get_versionless(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
        Ok((key == b"term").then(|| b"uid".to_vec()))
    }
}

impl Store for OneItem {
    type Snapshot = OneItemSnapshot;

    fn apply(&self, _: WriteBatch, _: PersistMode) -> Result<(), StoreError> {
        Ok(())
    }

    fn get_versionless(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
        Ok((key == b"term").then(|| b"uid".to_vec()))
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
    let expected = Item::new(b"k".to_vec(), 3, 0x08, false, b"v".to_vec());
    // (read timestamp, what get_at returns): the version at or below it, none below the version.
    for (read_ts, found) in [(u64::MAX, true), (5, true), (3, true), (2, false)] {
        assert_eq!(
            store.get_at(b"k", read_ts),
            Ok(found.then(|| expected.clone())),
            "read at {read_ts}"
        );
        assert_eq!(store.opened_at.get(), Some(read_ts));
    }
    assert_eq!(store.get_at(b"other", 5), Ok(None));
}

#[test]
fn test_get_at_passes_on_the_snapshots_refusal() {
    let store = OneItem::default();
    for read_ts in [1, 0] {
        let err = store.get_at(b"k", read_ts).unwrap_err();
        assert_eq!(err, StoreError::TsTooOld());
        assert_eq!(err.to_string(), "Transaction is too old");
        assert_eq!(store.opened_at.get(), Some(read_ts));
    }
}

#[test]
fn test_get_at_passes_on_the_reads_failure() {
    let store = OneItem::default();
    assert_eq!(
        store.get_at(b"bad", 5),
        Err(StoreError::ReadFailed(
            b"bad".to_vec(),
            "checksum".to_string()
        ))
    );
    assert_eq!(store.opened_at.get(), Some(5));
}

/// A store that keeps no temporary keyspaces and no rebuild records refuses them by name, and holds no record.
#[test]
fn test_a_store_without_temporary_keyspaces_or_records_refuses_them() {
    let store = OneItem::default();
    let temporaries = StoreError::Unsupported("temporary keyspaces");
    let records = StoreError::Unsupported("rebuild records");
    assert_eq!(
        temporaries.to_string(),
        "The store does not support temporary keyspaces"
    );
    assert_eq!(
        records.to_string(),
        "The store does not support rebuild records"
    );

    assert_eq!(store.create_temporary(), Err(temporaries.clone()));
    let t = Temporary::new(1);
    let mut batch = WriteBatch::new();
    batch.push(Entry::put(b"k".to_vec(), 1, 0, vec![]));
    assert_eq!(store.apply_temporary(&t, batch), Err(temporaries.clone()));
    assert!(matches!(store.temporary_snapshot(&t), Err(e) if e == temporaries));
    assert_eq!(store.drop_temporary(t), Err(temporaries));
    assert_eq!(
        store.write_rebuild_record(&RebuildRecord::new(1, vec![b"p".to_vec()])),
        Err(records)
    );
    assert_eq!(store.rebuild_records(), Ok(Vec::new()));
}
