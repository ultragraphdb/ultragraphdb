/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::panic::{AssertUnwindSafe, catch_unwind};

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
    assert!(!c.is_empty());
    assert_eq!(c.get(b"other"), None);
    c.remove(b"absent");
    assert_eq!(c.len(), 1, "removing an absent key removes nothing");
    c.remove(b"k");
    assert_eq!((c.get(b"k"), c.len()), (None, 0));
    assert!(c.is_empty());
}

#[test]
fn test_a_cache_fills_to_its_capacity_before_evicting() {
    let c = cache(3, 1);
    (0..3u8).for_each(|i| c.insert(vec![i], u64::from(i)));
    assert_eq!(
        (0..3u8).map(|i| c.get(&[i])).collect::<Vec<_>>(),
        [Some(0), Some(1), Some(2)]
    );
}

#[test]
fn test_a_full_cache_evicts_the_entry_its_stream_chooses() {
    // A full cache evicts the entry at index `rng.index(len)` of its ordered keys, the stream being the one handed
    // to it: replaying the stream by hand names the survivors.
    let survivors = |seed| {
        let c = cache(3, seed);
        (0..10u8).for_each(|i| c.insert(vec![i], u64::from(i)));
        assert_eq!(c.len(), 3);
        assert_eq!(c.get(&[9]), Some(9), "the entry just inserted stays");
        (0..10u8)
            .filter(|i| c.get(&[*i]).is_some())
            .collect::<Vec<u8>>()
    };
    for seed in 1..6 {
        let mut rng = Seed::new(seed).stream(Stream::Cache);
        let mut held: Vec<u8> = Vec::new();
        for i in 0..10u8 {
            if held.len() == 3 {
                held.remove(rng.index(3));
            }
            held.push(i);
        }
        assert_eq!(survivors(seed), held, "seed {seed}");
    }
    assert!((2..20).any(|seed| survivors(seed) != survivors(1)));
}

#[test]
fn test_replacing_an_entry_of_a_full_cache_evicts_nothing() {
    let c = cache(2, 1);
    c.insert(b"a".to_vec(), 1);
    c.insert(b"b".to_vec(), 2);
    c.insert(b"a".to_vec(), 3);
    assert_eq!((c.get(b"a"), c.get(b"b"), c.len()), (Some(3), Some(2), 2));
}

#[test]
fn test_a_cache_without_capacity_holds_nothing() {
    let c = cache(0, 1);
    c.insert(b"k".to_vec(), 1);
    assert_eq!((c.get(b"k"), c.len()), (None, 0));
    assert!(!c.evict_one());
}

#[test]
fn test_evict_one_removes_the_entry_its_stream_chooses() {
    let c = cache(4, 1);
    c.insert(b"a".to_vec(), 1);
    c.insert(b"b".to_vec(), 2);
    let at = Seed::new(1).stream(Stream::Cache).index(2);
    assert!(c.evict_one());
    let keys = [&b"a"[..], b"b"];
    assert_eq!(c.get(keys[at]), None);
    assert_eq!(c.get(keys[1 - at]), Some([1, 2][1 - at]));
    assert!(c.evict_one());
    assert!(c.is_empty());
    assert!(!c.evict_one(), "an empty cache evicts nothing");
}

/// A value whose clone panics on demand, to poison the cache's lock while it is held.
#[derive(Debug)]
struct Brittle(bool);

impl Clone for Brittle {
    fn clone(&self) -> Self {
        assert!(!self.0, "a brittle value breaks when cloned");
        Brittle(self.0)
    }
}

#[test]
fn test_a_panic_while_the_lock_is_held_leaves_the_cache_usable() {
    let c = SimListCache::new(4, Seed::new(1).stream(Stream::Cache));
    c.insert(b"brittle".to_vec(), Brittle(true));
    c.insert(b"sound".to_vec(), Brittle(false));
    let broke = catch_unwind(AssertUnwindSafe(|| c.get(b"brittle")));
    assert!(broke.is_err(), "the clone panicked inside the lock");
    assert_eq!(c.len(), 2);
    assert!(c.get(b"sound").is_some_and(|v| !v.0));
    c.remove(b"brittle");
    assert_eq!(c.len(), 1);
}
