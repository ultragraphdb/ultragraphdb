/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::StoreError;
use ultragraphdb_traits_store::{Item, Scan, StoreSnapshot};

use crate::types::corruption::Corruption;
use crate::types::op_record::OpRecord;
use crate::types::sim_scan::SimScan;
use crate::types::sim_snapshot::SimSnapshot;
use crate::types::sim_store::Inner;
use crate::utils::lock::lock;

impl SimSnapshot {
    /// Corrupts the version of `key` a read at this snapshot finds.
    fn corrupt_read_version(&self, inner: &mut Inner, key: &[u8], corruption: Corruption) {
        let Some((version, _)) = self.state.versions(key).find(|(v, _)| *v <= self.read_ts) else {
            return;
        };
        inner.corrupted.insert(key.to_vec(), version, corruption);
        inner.log.push(OpRecord::Corrupted {
            key: key.to_vec(),
            version,
            corruption,
        });
    }

    /// One key's read with its faults.
    fn faulty_read(&self, inner: &mut Inner, key: &[u8]) -> Result<Option<Item>, StoreError> {
        if !self.temporary
            && let Some(corruption) = inner.faults.corruption()
        {
            self.corrupt_read_version(inner, key, corruption);
        }
        if inner.faults.read_fails() {
            return Err(StoreError::ReadFailed(
                key.to_vec(),
                "simulated read failure".into(),
            ));
        }
        self.read(inner, key)
    }
}

impl StoreSnapshot for SimSnapshot {
    type Scan = SimScan;

    fn read_ts(&self) -> u64 {
        self.read_ts
    }

    fn get(&self, key: &[u8]) -> Result<Option<Item>, StoreError> {
        let mut inner = lock(&self.shared);
        inner.maybe_slow();
        let result = self.faulty_read(&mut inner, key);
        inner.log.push(OpRecord::Read {
            id: self.id,
            key: key.to_vec(),
            result: result.clone(),
        });
        result
    }

    fn get_many<K: AsRef<[u8]>>(&self, keys: &[K]) -> Result<Vec<Option<Item>>, StoreError> {
        let mut inner = lock(&self.shared);
        inner.maybe_slow();
        let result: Result<Vec<Option<Item>>, StoreError> = keys
            .iter()
            .map(|k| self.faulty_read(&mut inner, k.as_ref()))
            .collect();
        let keys = keys.iter().map(|k| k.as_ref().to_vec()).collect();
        inner.log.push(OpRecord::ReadMany {
            id: self.id,
            keys,
            result: result.clone(),
        });
        result
    }

    fn scan(&self, scan: &Scan) -> Self::Scan {
        let mut inner = lock(&self.shared);
        inner.maybe_slow();
        let selected: Vec<Vec<u8>> = self
            .state
            .range(scan)
            .map(|((key, _), _)| key.clone())
            .filter(|key| scan.selects(key))
            .collect();
        if !self.temporary
            && let Some(corruption) = inner.faults.corruption()
            && let Some(key) = selected
                .get(inner.faults.rng.index(selected.len()))
                .cloned()
        {
            self.corrupt_read_version(&mut inner, &key, corruption);
        }
        let mut items = self.scan_items(&inner, scan);
        // A scan that already ended at an undecodable version yields nothing after that error, the failure drawn
        // past it included.
        let ended = items.last().is_some_and(Result::is_err);
        if let Some(after) = inner.faults.scan_fails_after(items.len())
            && !(ended && after == items.len())
        {
            let key = items
                .get(after)
                .and_then(|i| i.as_ref().ok())
                .map_or_else(|| scan.start().to_vec(), |item| item.key().to_vec());
            items.truncate(after);
            items.push(Err(StoreError::ReadFailed(
                key,
                "simulated read failure partway through a scan".into(),
            )));
        }
        inner.log.push(OpRecord::Scanned {
            id: self.id,
            scan: scan.clone(),
            items: items.clone(),
        });
        SimScan::new(items)
    }
}
