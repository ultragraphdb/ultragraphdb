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
    cache.insert(b"j".to_vec(), Arc::new(vec![1, 2, 3]));
    cache.run_pending_tasks();
    assert_eq!(
        format!("{cache:?}"),
        "MokaListCache { entries: 1, weight: 3 }"
    );
}

#[test]
fn test_the_weigher_bounds_the_cache() {
    let cache: MokaListCache<u32> = MokaListCache::new(100, |key, _| key.len() as u32);
    (0..50u32).for_each(|i| cache.insert(vec![0; 10 + i as usize], i));
    cache.run_pending_tasks();
    // The weight is the weigher's sum over the entries the cache holds, and within the bound.
    let held: u64 = (0..50u32)
        .filter(|&i| cache.get(&vec![0; 10 + i as usize]) == Some(i))
        .map(|i| u64::from(10 + i))
        .sum();
    assert_eq!(cache.weighted_size(), held);
    assert!(held > 0 && held <= 100, "weight {held}");
    let heavy: MokaListCache<u32> = MokaListCache::new(100, |_, v| *v);
    heavy.insert(b"over".to_vec(), 101);
    heavy.run_pending_tasks();
    assert_eq!(
        heavy.get(b"over"),
        None,
        "an entry heavier than the bound is not kept"
    );
    assert_eq!(heavy.weighted_size(), 0);
    let clone = cache.clone();
    clone.insert(b"shared".to_vec(), 7);
    assert_eq!(cache.get(b"shared"), Some(7), "clones share one cache");
}
