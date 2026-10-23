/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;

use ultragraphdb_store_kv::MokaListCache;
use ultragraphdb_traits_store::ListCache;

#[test]
fn test_entries_are_inserted_replaced_and_removed() {
    let cache: MokaListCache<Arc<Vec<u8>>> =
        MokaListCache::new(1 << 20, |_, v: &Arc<Vec<u8>>| v.len() as u32);
    assert_eq!(cache.get(b"k"), None);
    cache.insert(b"k".to_vec(), Arc::new(vec![1]));
    cache.insert(b"k".to_vec(), Arc::new(vec![2]));
    assert_eq!(cache.get(b"k").as_deref(), Some(&vec![2]));
    cache.remove(b"k");
    assert_eq!(cache.get(b"k"), None);
    assert!(format!("{cache:?}").starts_with("MokaListCache"));
}

#[test]
fn test_the_weigher_bounds_the_cache() {
    let cache: MokaListCache<u32> = MokaListCache::new(100, |key, _| key.len() as u32);
    (0..50u32).for_each(|i| cache.insert(vec![0; 10 + i as usize], i));
    cache.run_pending_tasks();
    assert!(
        cache.weighted_size() <= 100,
        "weight {}",
        cache.weighted_size()
    );
    let clone = cache.clone();
    clone.insert(b"shared".to_vec(), 7);
    assert_eq!(cache.get(b"shared"), Some(7), "clones share one cache");
}
