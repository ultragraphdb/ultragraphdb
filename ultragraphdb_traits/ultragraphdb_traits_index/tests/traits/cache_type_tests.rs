/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_traits_index::CacheType;

use crate::utils::helpers::id;

/// A read-only store at one timestamp, each entity's value under its sorted key.
struct MapCache {
    ts: u64,
    values: BTreeMap<Vec<u8>, (SystemId, Vec<u8>)>,
}

impl CacheType for MapCache {
    type Error = String;

    fn get(&self, key: &[u8]) -> Result<Vec<u8>, Self::Error> {
        self.values
            .get(key)
            .map(|(_, v)| v.clone())
            .ok_or_else(|| "not found".to_string())
    }

    fn ts(&self) -> u64 {
        self.ts
    }

    fn find(
        &self,
        prefix: &[u8],
        filter: impl Fn(&[u8]) -> bool,
    ) -> Result<Option<SystemId>, Self::Error> {
        Ok(self
            .values
            .iter()
            .find(|(k, (_, v))| k.starts_with(prefix) && filter(v))
            .map(|(_, (id, _))| *id))
    }
}

#[test]
fn test_a_cache_reads_values_at_its_timestamp() -> TestResult {
    let c = MapCache {
        ts: 9,
        values: [
            (b"p1".to_vec(), (id(1)?, b"x".to_vec())),
            (b"p2".to_vec(), (id(2)?, b"y".to_vec())),
        ]
        .into(),
    };
    assert_eq!(c.ts(), 9);
    assert_eq!(c.get(b"p1"), Ok(b"x".to_vec()));
    assert!(c.get(b"q").is_err());
    assert_eq!(c.find(b"p", |v| v == b"y"), Ok(Some(id(2)?)));
    assert_eq!(c.find(b"p", |_| false), Ok(None), "none passes");
    Ok(())
}
