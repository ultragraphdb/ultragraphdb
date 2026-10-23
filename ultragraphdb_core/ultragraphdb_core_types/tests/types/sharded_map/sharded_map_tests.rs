/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_types::{NUM_SHARDS, ShardedMap, TypeId, Val, Value};

fn int(i: i64) -> Val {
    Val::new(TypeId::INT, Value::Int(i))
}

#[test]
fn test_sharded_map() {
    let mut a = ShardedMap::new();
    assert!(!a.is_empty());
    a.set(1, int(1));
    a.set(31, int(31));
    assert_eq!(a.len(), 2);
    assert_eq!(a.shard(1).len(), 2);
    assert_eq!(a.get(31), Some(&int(31)));
    assert_eq!(a.get(2), None);
    let mut b = ShardedMap::new();
    b.set(1, int(10));
    b.set(2, int(2));
    a.merge(&b, |x, y| match (x.value(), y.value()) {
        (Some(Value::Int(p)), Some(Value::Int(q))) => int(p + q),
        _ => x.clone(),
    });
    assert_eq!((a.len(), a.get(1)), (3, Some(&int(11))));
    let mut sum = 0;
    a.iterate(|k, _| -> Result<(), ()> {
        sum += k;
        Ok(())
    })
    .unwrap();
    assert_eq!(sum, 34);
    assert_eq!(a.iterate(|_, _| Err("stop")), Err("stop"));
    assert_eq!(NUM_SHARDS, 30);
}
