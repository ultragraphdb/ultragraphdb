/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_transport::{BufferPool, TransportError};
use ultragraphdb_tool_bdd::{Context, TestResult};

#[test]
fn test_a_pool_lends_its_buffers_and_takes_them_back() -> TestResult {
    let pool = BufferPool::new(2, 1_024);
    assert_eq!(
        (pool.buffers(), pool.capacity(), pool.available()),
        (2, 1_024, 2)
    );
    let a = pool.acquire()?;
    assert_eq!(pool.available(), 1);
    let b = pool.acquire()?;
    assert_eq!(pool.available(), 0);
    assert_eq!(
        pool.acquire().err().context("the refusal")?,
        TransportError::PoolExhausted(2)
    );
    drop(a);
    assert_eq!(pool.available(), 1);
    let c = pool.acquire()?;
    assert!(c.is_empty());
    assert!(c.capacity() >= 1_024);
    drop((b, c));
    assert_eq!(pool.available(), 2);
    // A clone is the same pool.
    let clone = pool.clone();
    let held = clone.acquire()?;
    assert_eq!(pool.available(), 1);
    drop(held);
    assert_eq!(pool.available(), 2);
    Ok(())
}

#[test]
fn test_a_pool_of_no_buffers_is_always_exhausted() -> TestResult {
    let pool = BufferPool::new(0, 64);
    assert_eq!(
        pool.acquire().err().context("the refusal")?,
        TransportError::PoolExhausted(0)
    );
    Ok(())
}
