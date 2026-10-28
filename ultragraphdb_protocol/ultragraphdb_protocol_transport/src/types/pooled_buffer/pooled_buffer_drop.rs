/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::buffer_pool::lock;
use crate::types::pooled_buffer::PooledBuffer;

impl Drop for PooledBuffer {
    /// Gives the buffer back to its pool, emptied.
    fn drop(&mut self) {
        let mut bytes = std::mem::take(&mut self.bytes);
        bytes.clear();
        lock(&self.pool).free.push(bytes);
    }
}
