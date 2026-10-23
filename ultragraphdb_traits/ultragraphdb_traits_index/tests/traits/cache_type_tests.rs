/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

use ultragraphdb_traits_index::CacheType;

/// A read-only store at one timestamp, values under sorted keys.
struct MapCache {
    ts: u64,
    values: BTreeMap<Vec<u8>, Vec<u8>>,
}

impl CacheType for MapCache {
    type Error = String;

    fn get(&self, key: &[u8]) -> Result<Vec<u8>, Self::Error> {
        self.values
            .get(key)
            .cloned()
            .ok_or_else(|| "not found".to_string())
    }

    fn ts(&self) -> u64 {
        self.ts
    }

    fn find(&self, prefix: &[u8], filter: impl Fn(&[u8]) -> bool) -> Result<u64, Self::Error> {
        self.values
            .iter()
            .filter(|(k, v)| k.starts_with(prefix) && filter(v))
            .map(|(k, _)| u64::from(*k.last().unwrap_or(&0)))
            .next()
            .ok_or_else(|| "none passes".to_string())
    }
}

#[test]
fn test_a_cache_reads_values_at_its_timestamp() {
    let c = MapCache {
        ts: 9,
        values: [
            (b"p1".to_vec(), b"x".to_vec()),
            (b"p2".to_vec(), b"y".to_vec()),
        ]
        .into(),
    };
    assert_eq!(c.ts(), 9);
    assert_eq!(c.get(b"p1"), Ok(b"x".to_vec()));
    assert!(c.get(b"q").is_err());
    assert_eq!(c.find(b"p", |v| v == b"y"), Ok(u64::from(b'2')));
    assert!(c.find(b"p", |_| false).is_err());
}
