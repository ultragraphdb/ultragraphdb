/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_sim::{Seed, SimListCache, Stream};
use ultragraphdb_traits_store::ListCache;

fn cache(capacity: usize, seed: u64) -> SimListCache<u64> {
    SimListCache::new(capacity, Seed::new(seed).stream(Stream::Cache))
}

#[test]
fn test_insert_get_and_remove() {
    let c = cache(4, 1);
    assert!(c.is_empty());
    c.insert(b"k".to_vec(), 1);
    c.insert(b"k".to_vec(), 2);
    assert_eq!((c.get(b"k"), c.len()), (Some(2), 1));
    c.remove(b"k");
    assert_eq!(c.get(b"k"), None);
}

#[test]
fn test_a_full_cache_evicts_one_entry_its_seed_chooses() {
    let survivors = |seed| {
        let c = cache(3, seed);
        (0..10u8).for_each(|i| c.insert(vec![i], u64::from(i)));
        assert_eq!(c.len(), 3);
        assert_eq!(c.get(&[9]), Some(9), "the entry just inserted stays");
        (0..10u8)
            .filter(|i| c.get(&[*i]).is_some())
            .collect::<Vec<u8>>()
    };
    assert_eq!(survivors(1), survivors(1));
    assert!((2..20).any(|seed| survivors(seed) != survivors(1)));
}

#[test]
fn test_replacing_an_entry_of_a_full_cache_evicts_nothing() {
    let c = cache(2, 1);
    c.insert(b"a".to_vec(), 1);
    c.insert(b"b".to_vec(), 2);
    c.insert(b"a".to_vec(), 3);
    assert_eq!((c.get(b"a"), c.get(b"b")), (Some(3), Some(2)));
}

#[test]
fn test_a_cache_without_capacity_holds_nothing() {
    let c = cache(0, 1);
    c.insert(b"k".to_vec(), 1);
    assert_eq!(c.get(b"k"), None);
    assert!(!c.evict_one());
}

#[test]
fn test_evict_one_removes_an_entry() {
    let c = cache(4, 1);
    c.insert(b"a".to_vec(), 1);
    c.insert(b"b".to_vec(), 2);
    assert!(c.evict_one());
    assert_eq!(c.len(), 1);
}
