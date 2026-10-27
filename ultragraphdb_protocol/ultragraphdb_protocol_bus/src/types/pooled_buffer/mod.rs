/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod pooled_buffer_drop;

use std::cell::RefCell;
use std::rc::Rc;

use crate::types::buffer_pool::PoolState;

/// A buffer taken from a [`crate::BufferPool`], given back to it when dropped.
#[derive(Debug)]
pub struct PooledBuffer {
    bytes: Vec<u8>,
    pool: Rc<RefCell<PoolState>>,
}

impl PooledBuffer {
    pub(crate) fn new(bytes: Vec<u8>, pool: Rc<RefCell<PoolState>>) -> Self {
        Self { bytes, pool }
    }

    /// The bytes read into it.
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Its capacity, the pool's.
    pub fn capacity(&self) -> usize {
        self.pool.borrow().capacity
    }

    /// The vector, taken out to be filled by an owned-buffer read; [`PooledBuffer::restore`] puts it back.
    pub(crate) fn take(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.bytes)
    }

    pub(crate) fn restore(&mut self, bytes: Vec<u8>) {
        self.bytes = bytes;
    }
}
