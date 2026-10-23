/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

use ultragraphdb_traits_index::LocalCache;

struct MapLocal(BTreeMap<Vec<u8>, Vec<u8>>);

impl LocalCache for MapLocal {
    type Error = String;

    fn get(&self, key: &[u8]) -> Result<Vec<u8>, Self::Error> {
        self.0
            .get(key)
            .cloned()
            .ok_or_else(|| "not found".to_string())
    }

    fn get_with_lock_held(&self, key: &[u8]) -> Result<Vec<u8>, Self::Error> {
        self.get(key)
    }

    fn find(&self, prefix: &[u8], filter: impl Fn(&[u8]) -> bool) -> Result<u64, Self::Error> {
        let n = self
            .0
            .iter()
            .filter(|(k, v)| k.starts_with(prefix) && filter(v))
            .count();
        Ok(n as u64)
    }
}

#[test]
fn test_a_local_cache_reads_with_and_without_the_lock() {
    let c = MapLocal(
        [
            (b"a".to_vec(), b"1".to_vec()),
            (b"ab".to_vec(), b"2".to_vec()),
        ]
        .into(),
    );
    assert_eq!(c.get(b"a"), Ok(b"1".to_vec()));
    assert_eq!(c.get_with_lock_held(b"ab"), Ok(b"2".to_vec()));
    assert!(c.get(b"b").is_err());
    assert_eq!(c.find(b"a", |v| v != b"1"), Ok(1));
}
