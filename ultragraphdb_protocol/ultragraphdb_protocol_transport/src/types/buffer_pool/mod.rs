/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::future::poll_fn;
use std::sync::PoisonError;
use std::task::{Poll, Waker};

use crate::errors::transport_error::TransportError;
use crate::types::pooled_buffer::PooledBuffer;
use crate::utils::sync::{Arc, Mutex, MutexGuard};

/// The pool's state, locked; a lock poisoned by a panic elsewhere still holds whole buffers.
pub(crate) fn lock(state: &Mutex<PoolState>) -> MutexGuard<'_, PoolState> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The free buffers of a pool, its sizes, and the tasks waiting for a buffer, shared by the pool and every buffer out of
/// it.
#[derive(Debug)]
pub(crate) struct PoolState {
    pub(crate) free: Vec<Vec<u8>>,
    pub(crate) buffers: usize,
    pub(crate) capacity: usize,
    /// Woken when a buffer is given back; one per task waiting, at most one per reader sharing the pool.
    pub(crate) waiting: Vec<Waker>,
}

/// A core's frame buffers, allocated once at start, as TigerBeetle's `MessagePool`: a frame is read into a buffer
/// taken from the pool, which takes it back when the buffer is dropped. A buffer an owned-buffer read took with it when
/// cancelled in flight is allocated again as it is given back, so every buffer the pool lends holds its capacity. One pool per core or per client; it is `Send`
/// and `Sync`, so a client on a multi-threaded runtime moves between threads with its frames, at the cost of one
/// uncontended lock per buffer taken and given back.
#[derive(Debug, Clone)]
pub struct BufferPool {
    state: Arc<Mutex<PoolState>>,
}

impl BufferPool {
    /// A pool of `buffers` buffers of `capacity` bytes each, allocated now.
    pub fn new(buffers: usize, capacity: usize) -> Self {
        Self {
            state: Arc::new(Mutex::new(PoolState {
                free: (0..buffers).map(|_| Vec::with_capacity(capacity)).collect(),
                buffers,
                capacity,
                waiting: Vec::new(),
            })),
        }
    }

    /// A free buffer, empty, refused as [`TransportError::pool_exhausted`] when every buffer is in use.
    pub fn acquire(&self) -> Result<PooledBuffer, TransportError> {
        let mut state = lock(&self.state);
        match state.free.pop() {
            Some(bytes) => Ok(PooledBuffer::new(
                bytes,
                state.capacity,
                Arc::clone(&self.state),
            )),
            None => Err(TransportError::pool_exhausted(state.buffers)),
        }
    }

    /// A free buffer, empty, once one is: while every buffer is in use the wait lasts until one is given back, the
    /// backpressure of a reader whose frames are all held. The caller bounds the wait, as it bounds the read it is for.
    /// A future dropped at its wait takes no buffer, and its waker stays registered until the next buffer given back
    /// wakes it, once.
    pub async fn acquire_waiting(&self) -> PooledBuffer {
        poll_fn(|cx| {
            let mut state = lock(&self.state);
            match state.free.pop() {
                Some(bytes) => Poll::Ready(PooledBuffer::new(
                    bytes,
                    state.capacity,
                    Arc::clone(&self.state),
                )),
                None => {
                    if !state.waiting.iter().any(|w| w.will_wake(cx.waker())) {
                        state.waiting.push(cx.waker().clone());
                    }
                    Poll::Pending
                }
            }
        })
        .await
    }

    /// The buffers not in use.
    pub fn available(&self) -> usize {
        lock(&self.state).free.len()
    }

    /// The buffers the pool holds, in use or not.
    pub fn buffers(&self) -> usize {
        lock(&self.state).buffers
    }

    /// Each buffer's capacity in bytes.
    pub fn capacity(&self) -> usize {
        lock(&self.state).capacity
    }
}
