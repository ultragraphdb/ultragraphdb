/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;

use ultragraphdb_error_store::StoreError;
use ultragraphdb_traits_store::{Item, Scan, StoreSnapshot};

use crate::types::counted_snapshot::{CountedScan, CountedSnapshot};

impl StoreSnapshot for CountedSnapshot {
    type Scan = CountedScan;

    fn read_ts(&self) -> u64 {
        self.inner.read_ts()
    }

    fn get(&self, key: &[u8]) -> Result<Option<Item>, StoreError> {
        self.inner.get(key)
    }

    fn get_many<K: AsRef<[u8]>>(&self, keys: &[K]) -> Result<Vec<Option<Item>>, StoreError> {
        self.inner.get_many(keys)
    }

    fn scan(&self, scan: &Scan) -> Self::Scan {
        CountedScan {
            inner: self.inner.scan(scan),
            empty: None,
        }
    }

    fn get_versionless(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
        self.inner.get_versionless(key)
    }

    fn versions(&self, key: &[u8]) -> Self::Scan {
        self.counts.store_read();
        CountedScan {
            inner: self.inner.versions(key),
            empty: Some(Arc::clone(&self.counts)),
        }
    }
}
