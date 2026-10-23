/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use ultragraphdb_traits_index::{KeyValue, Txn};

/// A transaction over a map, with its own lock set.
#[derive(Default)]
struct MapTxn {
    values: RefCell<BTreeMap<Vec<u8>, KeyValue>>,
    locked: RefCell<BTreeSet<Vec<u8>>>,
}

impl Txn for MapTxn {
    type Error = String;

    fn start_ts(&self) -> u64 {
        7
    }

    fn get(&self, key: &[u8]) -> Result<Vec<u8>, Self::Error> {
        self.values
            .borrow()
            .get(key)
            .map(|kv| kv.value().to_vec())
            .ok_or_else(|| "not found".to_string())
    }

    fn get_with_lock_held(&self, key: &[u8]) -> Result<Vec<u8>, Self::Error> {
        if !self.locked.borrow().contains(key) {
            return Err("not locked".to_string());
        }
        self.get(key)
    }

    fn find(&self, prefix: &[u8], filter: impl Fn(&[u8]) -> bool) -> Result<u64, Self::Error> {
        self.values
            .borrow()
            .iter()
            .find(|(k, kv)| k.starts_with(prefix) && filter(kv.value()))
            .map(|(_, kv)| kv.entity())
            .ok_or_else(|| "none passes".to_string())
    }

    fn add_mutation(&self, key: &[u8], t: &KeyValue) -> Result<(), Self::Error> {
        self.values.borrow_mut().insert(key.to_vec(), t.clone());
        Ok(())
    }

    fn add_mutation_with_lock_held(&self, key: &[u8], t: &KeyValue) -> Result<(), Self::Error> {
        if !self.locked.borrow().contains(key) {
            return Err("not locked".to_string());
        }
        self.add_mutation(key, t)
    }

    fn lock_key(&self, key: &[u8]) {
        self.locked.borrow_mut().insert(key.to_vec());
    }

    fn unlock_key(&self, key: &[u8]) {
        self.locked.borrow_mut().remove(key);
    }
}

#[test]
fn test_a_txn_reads_its_writes_and_honours_its_locks() {
    let t = MapTxn::default();
    assert_eq!(t.start_ts(), 7);
    t.add_mutation(b"k1", &KeyValue::new(1, "a", b"v1".to_vec()))
        .unwrap();
    assert_eq!(t.get(b"k1"), Ok(b"v1".to_vec()));
    let kv = KeyValue::new(2, "a", b"v2".to_vec());
    assert!(t.add_mutation_with_lock_held(b"k2", &kv).is_err());
    t.lock_key(b"k2");
    t.add_mutation_with_lock_held(b"k2", &kv).unwrap();
    assert_eq!(t.get_with_lock_held(b"k2"), Ok(b"v2".to_vec()));
    t.unlock_key(b"k2");
    assert!(t.get_with_lock_held(b"k2").is_err());
    assert_eq!(t.find(b"k", |v| v == b"v2"), Ok(2));
}
