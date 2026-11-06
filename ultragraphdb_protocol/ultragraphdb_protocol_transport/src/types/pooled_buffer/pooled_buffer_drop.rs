/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::buffer_pool::lock;
use crate::types::pooled_buffer::PooledBuffer;

impl Drop for PooledBuffer {
    /// Gives the buffer back to its pool, emptied and at the pool's capacity, and wakes the tasks waiting for one, outside the pool's lock.
    fn drop(&mut self) {
        let mut bytes = std::mem::take(&mut self.bytes);
        bytes.clear();
        if bytes.capacity() < self.capacity {
            // Its vector went with a read cancelled in flight, which the driver keeps until the read completes.
            bytes = Vec::with_capacity(self.capacity);
        }
        let waiting = {
            let mut state = lock(&self.pool);
            state.free.push(bytes);
            std::mem::take(&mut state.waiting)
        };
        waiting.into_iter().for_each(std::task::Waker::wake);
    }
}
