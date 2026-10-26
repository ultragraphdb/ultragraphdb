/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;

use ultragraphdb_store_kv::MokaListCache;
use ultragraphdb_tool_bench::{CountingCache, ReadCounts};
use ultragraphdb_traits_store::ListCache;

#[test]
fn test_every_lookup_is_counted_and_the_entries_it_found() {
    let counts = Arc::new(ReadCounts::new());
    let cache = CountingCache::new(
        MokaListCache::new(1 << 20, |_: &[u8], _: &u64| 1),
        Arc::clone(&counts),
    );
    assert_eq!(cache.get(b"a"), None);
    cache.insert(b"a".to_vec(), 7);
    assert_eq!(cache.get(b"a"), Some(7));
    cache.remove(b"a");
    assert_eq!(cache.get(b"a"), None);
    // Writes to the cache are no lookups.
    assert_eq!(counts.get(), (3, 1, 0, 0));
}
