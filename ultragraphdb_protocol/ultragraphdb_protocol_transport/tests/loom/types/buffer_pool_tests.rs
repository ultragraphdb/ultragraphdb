/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use loom::thread;
use ultragraphdb_protocol_transport::BufferPool;

/// Wakes the Loom thread that waits on a future.
struct Unpark(thread::Thread);

impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

/// Runs `future` to its end on this Loom thread, parked between polls until its waker unparks it, so Loom sees every
/// wait and every wakeup.
fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(Unpark(thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
        thread::park();
    }
}

/// Two threads taking from a pool of one buffer at once: the buffer is lent once, the other is refused, and with both
/// ended the pool holds its one buffer again.
#[test]
fn test_two_taking_the_last_buffer_lend_it_once() {
    loom::model(|| {
        let pool = BufferPool::new(1, 64);
        let other = pool.clone();
        let theirs = thread::spawn(move || other.acquire());
        let mine = pool.acquire();
        let theirs = theirs.join();
        assert!(theirs.is_ok(), "the other thread's taking");
        let lent = [mine.is_ok(), theirs.as_ref().is_ok_and(Result::is_ok)];
        assert_eq!(lent.iter().filter(|l| **l).count(), 1, "{lent:?}");
        drop((mine, theirs));
        assert_eq!(pool.available(), 1);
    });
}

/// A buffer given back on one thread while another takes: the taker gets a buffer or is refused, never a second
/// buffer the pool does not hold, and the pool ends holding every buffer it began with.
#[test]
fn test_a_buffer_given_back_as_another_is_taken_is_counted_once() {
    loom::model(|| {
        let pool = BufferPool::new(2, 64);
        let (first, second) = (pool.acquire(), pool.acquire());
        assert!(first.is_ok() && second.is_ok());
        let giving = thread::spawn(move || drop(first));
        let third = pool.acquire();
        let out = usize::from(third.is_ok()) + 1;
        assert!(
            pool.available() + out <= 2,
            "more buffers than the pool holds"
        );
        assert!(giving.join().is_ok(), "the giving thread");
        drop((second, third));
        assert_eq!(pool.available(), 2);
    });
}

/// A task waiting for a buffer while the last one is given back on another thread: whether the wait begins before
/// the buffer returns or after, the task is woken and takes it, so no interleaving loses the wakeup.
#[test]
fn test_a_task_waiting_for_a_buffer_is_woken_when_one_is_given_back() {
    loom::model(|| {
        let pool = BufferPool::new(1, 64);
        let held = pool.acquire();
        assert!(held.is_ok());
        let waiter = pool.clone();
        let waiting = thread::spawn(move || {
            let buffer = block_on(waiter.acquire_waiting());
            buffer.capacity()
        });
        drop(held);
        assert!(waiting.join().is_ok_and(|capacity| capacity == 64));
        assert_eq!(pool.available(), 1);
    });
}
