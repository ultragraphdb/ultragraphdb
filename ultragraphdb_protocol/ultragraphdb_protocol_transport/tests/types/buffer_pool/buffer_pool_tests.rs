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
        TransportError::pool_exhausted(2)
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
        TransportError::pool_exhausted(0)
    );
    Ok(())
}

/// A waker counting its wakes.
struct Counted(std::sync::atomic::AtomicUsize);

impl std::task::Wake for Counted {
    fn wake(self: std::sync::Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &std::sync::Arc<Self>) {
        let _woken = self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// A full pool is backpressure: waiting for a buffer is pending while every buffer is in use, the buffer given back wakes
/// the waiter once, and the wait then takes it.
#[test]
fn test_waiting_for_a_buffer_ends_when_one_is_given_back() -> TestResult {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::{Context as TaskContext, Poll, Waker};
    let pool = BufferPool::new(1, 64);
    let free = std::pin::pin!(pool.acquire_waiting());
    let counted = Arc::new(Counted(AtomicUsize::new(0)));
    let waker = Waker::from(Arc::clone(&counted));
    let mut cx = TaskContext::from_waker(&waker);
    let Poll::Ready(held) = free.poll(&mut cx) else {
        return Err(ultragraphdb_tool_bdd::TestError::new(
            "a free buffer is taken at once",
        ));
    };
    let mut waiting = std::pin::pin!(pool.acquire_waiting());
    assert!(
        waiting.as_mut().poll(&mut cx).is_pending(),
        "no buffer is free"
    );
    assert!(
        waiting.as_mut().poll(&mut cx).is_pending(),
        "polled again, still none"
    );
    assert_eq!(counted.0.load(Ordering::Relaxed), 0);
    drop(held);
    assert_eq!(counted.0.load(Ordering::Relaxed), 1, "woken once");
    let Poll::Ready(taken) = waiting.as_mut().poll(&mut cx) else {
        return Err(ultragraphdb_tool_bdd::TestError::new(
            "the buffer given back is taken",
        ));
    };
    assert!(taken.is_empty());
    assert_eq!(pool.available(), 0);
    drop(taken);
    assert_eq!(pool.available(), 1);
    Ok(())
}
