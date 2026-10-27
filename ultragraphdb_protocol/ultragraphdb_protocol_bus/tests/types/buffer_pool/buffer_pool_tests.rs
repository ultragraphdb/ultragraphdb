/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_bus::{BufferPool, BusError};

#[test]
fn test_a_pool_lends_its_buffers_and_takes_them_back() {
    let pool = BufferPool::new(2, 1_024);
    assert_eq!(
        (pool.buffers(), pool.capacity(), pool.available()),
        (2, 1_024, 2)
    );
    let a = pool.acquire().unwrap();
    assert_eq!(pool.available(), 1);
    let b = pool.acquire().unwrap();
    assert_eq!(pool.available(), 0);
    assert_eq!(pool.acquire().unwrap_err(), BusError::PoolExhausted(2));
    drop(a);
    assert_eq!(pool.available(), 1);
    let c = pool.acquire().unwrap();
    assert!(c.is_empty());
    assert!(c.capacity() >= 1_024);
    drop((b, c));
    assert_eq!(pool.available(), 2);
    // A clone is the same pool.
    let clone = pool.clone();
    let held = clone.acquire().unwrap();
    assert_eq!(pool.available(), 1);
    drop(held);
    assert_eq!(pool.available(), 2);
}

#[test]
fn test_a_pool_of_no_buffers_is_always_exhausted() {
    let pool = BufferPool::new(0, 64);
    assert_eq!(pool.acquire().unwrap_err(), BusError::PoolExhausted(0));
}
