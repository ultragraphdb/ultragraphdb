/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Dgraph's `ShardedMap` (types/sharded_map.go).

use ultragraphdb_core_types::{NUM_SHARDS, ShardedMap, TypeId, Val, Value};

fn int(i: i64) -> Val {
    Val::new(TypeId::INT, Value::Int(i))
}

fn sum(x: &Val, y: &Val) -> Val {
    match (x.value(), y.value()) {
        (Some(Value::Int(p)), Some(Value::Int(q))) => int(p * 1000 + q),
        _ => panic!("{x:?} {y:?}"),
    }
}

#[test]
fn test_a_new_map_has_thirty_empty_shards() {
    assert_eq!(NUM_SHARDS, 30);
    for m in [ShardedMap::new(), ShardedMap::default()] {
        // Go's IsEmpty asks whether the map has shards, not whether it holds values.
        assert!(!m.is_empty());
        assert_eq!(m.len(), 0);
        assert_eq!(m.get(0), None);
        for i in 0..NUM_SHARDS {
            assert_eq!(m.shard(i).map(|s| s.len()), Some(0), "shard {i}");
        }
        // Go indexes past the shards and panics.
        assert_eq!(m.shard(NUM_SHARDS), None);
        assert_eq!(m.shard(usize::MAX), None);
    }
}

#[test]
fn test_a_key_lives_in_shard_key_mod_thirty() {
    let mut m = ShardedMap::new();
    // u64::MAX % 30 is 15.
    let keys = [0, 1, 29, 30, 31, 59, 60, u64::MAX];
    let shards = [0, 1, 29, 0, 1, 29, 0, 15];
    for (k, i) in keys.iter().zip(1..) {
        m.set(*k, int(i));
    }
    assert_eq!(m.len(), keys.len());
    for ((k, s), i) in keys.iter().zip(shards).zip(1..) {
        assert_eq!(m.get(*k), Some(&int(i)), "key {k}");
        assert_eq!(
            m.shard(s).and_then(|sh| sh.get(k)),
            Some(&int(i)),
            "key {k}"
        );
    }
    let sizes: Vec<usize> = (0..NUM_SHARDS).map(|i| m.shard(i).unwrap().len()).collect();
    let mut want = vec![0; NUM_SHARDS];
    (want[0], want[1], want[15], want[29]) = (3, 2, 1, 2);
    assert_eq!(sizes, want);
    assert_eq!(m.get(2), None);
    assert_eq!(m.get(u64::MAX - 30), None);
    // Set replaces.
    m.set(31, int(-1));
    assert_eq!((m.len(), m.get(31)), (keys.len(), Some(&int(-1))));
}

#[test]
fn test_merge() {
    let mut a = ShardedMap::new();
    a.set(1, int(1));
    a.set(31, int(31));
    let mut b = ShardedMap::new();
    b.set(1, int(10));
    b.set(2, int(2));
    b.set(61, int(61));
    // ag sees the receiver's value first.
    a.merge(&b, sum);
    assert_eq!(a.len(), 4);
    assert_eq!(a.get(1), Some(&int(1010)));
    assert_eq!(a.get(31), Some(&int(31)));
    assert_eq!(a.get(2), Some(&int(2)));
    assert_eq!(a.get(61), Some(&int(61)));
    // The other map is unchanged.
    assert_eq!((b.len(), b.get(1)), (3, Some(&int(10))));
    // Merging an empty map changes nothing; merging into an empty map copies.
    let before = a.clone();
    a.merge(&ShardedMap::new(), |_, _| panic!("no key in both"));
    assert_eq!(a, before);
    let mut c = ShardedMap::new();
    c.merge(&b, |_, _| panic!("no key in both"));
    assert_eq!(c, b);
}

#[test]
fn test_iterate_goes_shard_by_shard_and_stops_at_the_first_error() {
    let mut m = ShardedMap::new();
    // One key in each of shards 29, 0 and 1.
    for k in [29, 30, 1] {
        m.set(k, int(k as i64));
    }
    let mut seen = Vec::new();
    assert_eq!(
        m.iterate(|k, v| -> Result<(), ()> {
            assert_eq!(v, &int(k as i64));
            seen.push(k);
            Ok(())
        }),
        Ok(())
    );
    assert_eq!(seen, [30, 1, 29]);
    let mut calls = 0;
    assert_eq!(
        m.iterate(|k, _| {
            calls += 1;
            if k == 1 { Err(k) } else { Ok(()) }
        }),
        Err(1)
    );
    assert_eq!(calls, 2);
    let mut none = 0;
    assert_eq!(
        ShardedMap::new().iterate(|_, _| -> Result<(), ()> {
            none += 1;
            Ok(())
        }),
        Ok(())
    );
    assert_eq!(none, 0);
}
