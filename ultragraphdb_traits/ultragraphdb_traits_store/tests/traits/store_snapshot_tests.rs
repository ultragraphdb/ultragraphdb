/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;

use ultragraphdb_error_store::StoreError;
use ultragraphdb_traits_store::{Direction, Item, Scan, StoreSnapshot};

/// A snapshot recording the scans it is asked for and answering each with one item per selected candidate.
#[derive(Default)]
struct Recorder {
    scans: RefCell<Vec<Scan>>,
}

/// The keys the recorder holds, each at version 3.
const KEYS: [&[u8]; 5] = [b"", b"\x00", b"k", b"k\x00", b"k\xff"];

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
        KEYS.iter()
            .filter(|k| scan.selects(k))
            .map(|k| Ok(Item::new(k.to_vec(), 3, 0, false, vec![])))
            .collect::<Vec<_>>()
            .into_iter()
    }
}

#[test]
fn test_versions_is_a_forward_all_versions_scan_of_the_one_key() {
    for key in KEYS {
        let snapshot = Recorder::default();
        let items: Vec<_> = snapshot.versions(key).collect();
        assert_eq!(
            items,
            vec![Ok(Item::new(key.to_vec(), 3, 0, false, vec![]))],
            "versions of {key:?}"
        );
        let scans = snapshot.scans.borrow();
        assert_eq!(scans.as_slice(), &[Scan::key(key.to_vec()).all_versions()]);
        // The one key's range: from the key itself to the key followed by 0x00, exclusive.
        let mut end = key.to_vec();
        end.push(0x00);
        assert_eq!(
            (scans[0].start(), scans[0].end()),
            (key, Some(end.as_slice()))
        );
        assert_eq!(
            (
                scans[0].direction(),
                scans[0].is_all_versions(),
                scans[0].seek_key()
            ),
            (Direction::Forward, true, None)
        );
    }
}

#[test]
fn test_versions_of_an_absent_key_is_empty() {
    let snapshot = Recorder::default();
    assert_eq!(snapshot.versions(b"j").count(), 0);
    assert_eq!(
        snapshot.scans.borrow().as_slice(),
        &[Scan::range(b"j".to_vec(), Some(b"j\x00".to_vec())).all_versions()]
    );
}
