/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::ops::Bound;

use fjall::Readable;
use ultragraphdb_error_store::StoreError;
use ultragraphdb_traits_store::{Direction, Item, Scan, StoreSnapshot};

use crate::types::fjall_scan::FjallScan;
use crate::types::fjall_snapshot::FjallSnapshot;
use crate::utils::stored_key::{escape, key_end, stored_key};
use crate::utils::stored_value::decode_value;

impl StoreSnapshot for FjallSnapshot {
    type Scan = FjallScan;

    fn read_ts(&self) -> u64 {
        self.read_ts
    }

    /// One seek to the key's newest version at or below the read timestamp, through a merging iterator rather than
    /// a point get, as Dgraph's `mvcc.go:858` reads: a rollup can write a version below versions in higher levels.
    fn get(&self, key: &[u8]) -> Result<Option<Item>, StoreError> {
        if self.banned(key) {
            return Ok(None);
        }
        let range = (
            Bound::Included(stored_key(key, self.read_ts)),
            Bound::Excluded(key_end(key)),
        );
        let Some(guard) = self.snapshot.range(&self.shared.data, range).next() else {
            return Ok(None);
        };
        let (stored, bytes) = guard
            .into_inner()
            .map_err(|e| StoreError::ReadFailed(key.to_vec(), e.to_string()))?;
        let mut raw = [0u8; 8];
        raw.copy_from_slice(&stored[stored.len().saturating_sub(8)..]);
        let version = !u64::from_be_bytes(raw);
        let v =
            decode_value(&bytes).map_err(|reason| StoreError::Corrupted(key.to_vec(), reason))?;
        Ok((!v.deletion)
            .then(|| Item::new(key.to_vec(), version, v.meta, v.discard, v.value.to_vec())))
    }

    fn get_many<K: AsRef<[u8]>>(&self, keys: &[K]) -> Result<Vec<Option<Item>>, StoreError> {
        keys.iter().map(|k| self.get(k.as_ref())).collect()
    }

    fn scan(&self, scan: &Scan) -> Self::Scan {
        let mut lower = escape(scan.start());
        let mut upper = scan.end().map(escape);
        match (scan.seek_key(), scan.direction()) {
            (Some(seek), Direction::Forward) => lower = lower.max(escape(seek)),
            (Some(seek), Direction::Reverse) => {
                let end = key_end(seek);
                upper = Some(upper.map_or(end.clone(), |u| u.min(end)));
            }
            (None, _) => {}
        }
        if upper.as_ref().is_some_and(|u| *u <= lower) {
            return FjallScan::empty();
        }
        let range = (
            Bound::Included(lower),
            upper.map_or(Bound::Unbounded, Bound::Excluded),
        );
        let iter = self.snapshot.range(&self.shared.data, range);
        FjallScan::new(iter, scan.clone(), self.read_ts, self.banned.clone())
    }
}
