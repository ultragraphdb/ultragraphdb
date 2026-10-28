/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::errors::transport_error::TransportError;
use crate::types::pooled_buffer::PooledBuffer;

/// The pool's state, locked; a lock poisoned by a panic elsewhere still holds whole buffers.
pub(crate) fn lock(state: &Mutex<PoolState>) -> MutexGuard<'_, PoolState> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The free buffers of a pool and its sizes, shared by the pool and every buffer out of it.
#[derive(Debug)]
pub(crate) struct PoolState {
    pub(crate) free: Vec<Vec<u8>>,
    pub(crate) buffers: usize,
    pub(crate) capacity: usize,
}

/// A core's frame buffers, allocated once at start, as TigerBeetle's `MessagePool`: a frame is read into a buffer
/// taken from the pool, which takes it back when the buffer is dropped. One pool per core or per client; it is `Send`
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
            })),
        }
    }

    /// A free buffer, empty, refused as [`TransportError::PoolExhausted`] when every buffer is in use.
    pub fn acquire(&self) -> Result<PooledBuffer, TransportError> {
        let mut state = lock(&self.state);
        match state.free.pop() {
            Some(bytes) => Ok(PooledBuffer::new(bytes, Arc::clone(&self.state))),
            None => Err(TransportError::PoolExhausted(state.buffers)),
        }
    }

    /// The buffers not in use.
    pub fn available(&self) -> usize {
        lock(&self.state).free.len()
    }

    pub fn buffers(&self) -> usize {
        lock(&self.state).buffers
    }

    /// Each buffer's capacity in bytes.
    pub fn capacity(&self) -> usize {
        lock(&self.state).capacity
    }
}
