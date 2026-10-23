/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use ultragraphdb_traits_store::ListCache;

/// A cache that never evicts.
#[derive(Default)]
struct Keep(Mutex<BTreeMap<Vec<u8>, Arc<u64>>>);

impl ListCache for Keep {
    type Value = Arc<u64>;

    fn get(&self, key: &[u8]) -> Option<Self::Value> {
        self.0.lock().unwrap().get(key).cloned()
    }

    fn insert(&self, key: Vec<u8>, value: Self::Value) {
        self.0.lock().unwrap().insert(key, value);
    }

    fn remove(&self, key: &[u8]) {
        self.0.lock().unwrap().remove(key);
    }
}

#[test]
fn test_insert_replaces_and_remove_forgets() {
    let cache = Keep::default();
    cache.insert(b"k".to_vec(), Arc::new(1));
    cache.insert(b"k".to_vec(), Arc::new(2));
    assert_eq!(cache.get(b"k").as_deref(), Some(&2));
    cache.remove(b"k");
    assert_eq!(cache.get(b"k"), None);
}
