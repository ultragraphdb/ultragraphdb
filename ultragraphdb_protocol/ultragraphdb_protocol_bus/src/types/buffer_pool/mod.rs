/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;
use std::rc::Rc;

use crate::errors::bus_error::BusError;
use crate::types::pooled_buffer::PooledBuffer;

/// The free buffers of a pool and its sizes, shared by the pool and every buffer out of it.
#[derive(Debug)]
pub(crate) struct PoolState {
    pub(crate) free: Vec<Vec<u8>>,
    pub(crate) buffers: usize,
    pub(crate) capacity: usize,
}

/// A core's frame buffers, allocated once at start, as TigerBeetle's `MessagePool`: a frame is read into a buffer
/// taken from the pool, which takes it back when the buffer is dropped. One pool per core: it is not `Send`.
#[derive(Debug, Clone)]
pub struct BufferPool {
    state: Rc<RefCell<PoolState>>,
}

impl BufferPool {
    /// A pool of `buffers` buffers of `capacity` bytes each, allocated now.
    pub fn new(buffers: usize, capacity: usize) -> Self {
        Self {
            state: Rc::new(RefCell::new(PoolState {
                free: (0..buffers).map(|_| Vec::with_capacity(capacity)).collect(),
                buffers,
                capacity,
            })),
        }
    }

    /// A free buffer, empty, refused as [`BusError::PoolExhausted`] when every buffer is in use.
    pub fn acquire(&self) -> Result<PooledBuffer, BusError> {
        let mut state = self.state.borrow_mut();
        match state.free.pop() {
            Some(bytes) => Ok(PooledBuffer::new(bytes, Rc::clone(&self.state))),
            None => Err(BusError::PoolExhausted(state.buffers)),
        }
    }

    /// The buffers not in use.
    pub fn available(&self) -> usize {
        self.state.borrow().free.len()
    }

    pub fn buffers(&self) -> usize {
        self.state.borrow().buffers
    }

    /// Each buffer's capacity in bytes.
    pub fn capacity(&self) -> usize {
        self.state.borrow().capacity
    }
}
