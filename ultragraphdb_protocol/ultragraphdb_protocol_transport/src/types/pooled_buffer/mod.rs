/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod pooled_buffer_drop;

use std::sync::{Arc, Mutex};

use crate::types::buffer_pool::{PoolState, lock};

/// A buffer taken from a [`crate::BufferPool`], given back to it when dropped.
#[derive(Debug)]
pub struct PooledBuffer {
    bytes: Vec<u8>,
    pool: Arc<Mutex<PoolState>>,
}

impl PooledBuffer {
    pub(crate) fn new(bytes: Vec<u8>, pool: Arc<Mutex<PoolState>>) -> Self {
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
        lock(&self.pool).capacity
    }

    /// The vector, taken out for a shell to read into: an owned-buffer read (io_uring) fills it and hands it back, a
    /// `&mut [u8]` read fills its spare capacity; [`PooledBuffer::restore`] puts it back. Its capacity is the pool's.
    pub fn take(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.bytes)
    }

    /// Puts back the vector [`PooledBuffer::take`] took out, holding the bytes read into it.
    pub fn restore(&mut self, bytes: Vec<u8>) {
        self.bytes = bytes;
    }
}
