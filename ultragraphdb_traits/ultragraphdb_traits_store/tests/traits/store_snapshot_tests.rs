/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;

use ultragraphdb_error_store::StoreError;
use ultragraphdb_traits_store::{Item, Scan, StoreSnapshot};

/// A snapshot recording the scans it is asked for.
#[derive(Default)]
struct Recorder {
    scans: RefCell<Vec<Scan>>,
}

impl StoreSnapshot for Recorder {
    type Scan = std::vec::IntoIter<Result<Item, StoreError>>;

    fn read_ts(&self) -> u64 {
        4
    }

    fn get(&self, _: &[u8]) -> Result<Option<Item>, StoreError> {
        Ok(None)
    }

    fn get_many<K: AsRef<[u8]>>(&self, keys: &[K]) -> Result<Vec<Option<Item>>, StoreError> {
        Ok(keys.iter().map(|_| None).collect())
    }

    fn scan(&self, scan: &Scan) -> Self::Scan {
        self.scans.borrow_mut().push(scan.clone());
        vec![Ok(Item::new(scan.start().to_vec(), 3, 0, false, vec![]))].into_iter()
    }
}

#[test]
fn test_versions_is_a_scan_of_the_one_key_with_all_versions() {
    let snapshot = Recorder::default();
    let items: Vec<_> = snapshot.versions(b"k").collect();
    assert_eq!(items.len(), 1);
    assert_eq!(
        snapshot.scans.borrow().as_slice(),
        &[Scan::key(b"k".to_vec()).all_versions()]
    );
    assert_eq!(snapshot.read_ts(), 4);
    assert_eq!(snapshot.get_many(&[b"a", b"b"]).unwrap(), vec![None, None]);
}
