/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

/// The maximum frame, 1 MiB: the default bound of a write-set record and of a read set.
pub const MAX_FRAME: usize = 1 << 20;

/// A transaction's limits, each a setting: how long a read-write and a read-only transaction live, the largest
/// write-set record and read set it may hold, and how many objects of one list its write set names one by one before
/// it names the list whole.
///
/// | Setting | Default |
/// |---|---|
/// | `read_write` | 10 s |
/// | `read_only` | 16 s |
/// | `max_write_set` | 1 MiB, the maximum frame |
/// | `max_read_set` | 1 MiB |
/// | `objects_per_list` | 64 |
///
/// A byte bound above `u32::MAX` is taken as `u32::MAX`: a read set's and a record's offsets are u32.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TxnLimits {
    read_write: Duration,
    read_only: Duration,
    max_write_set: usize,
    max_read_set: usize,
    objects_per_list: usize,
}

impl Default for TxnLimits {
    fn default() -> Self {
        Self {
            read_write: Duration::from_secs(10),
            read_only: Duration::from_secs(16),
            max_write_set: MAX_FRAME,
            max_read_set: MAX_FRAME,
            objects_per_list: 64,
        }
    }
}

/// `bytes`, at most `u32::MAX`.
fn capped(bytes: usize) -> usize {
    bytes.min(u32::MAX as usize)
}

impl TxnLimits {
    pub fn with_read_write(self, limit: Duration) -> Self {
        Self {
            read_write: limit,
            ..self
        }
    }

    pub fn with_read_only(self, limit: Duration) -> Self {
        Self {
            read_only: limit,
            ..self
        }
    }

    pub fn with_max_write_set(self, bytes: usize) -> Self {
        Self {
            max_write_set: capped(bytes),
            ..self
        }
    }

    pub fn with_max_read_set(self, bytes: usize) -> Self {
        Self {
            max_read_set: capped(bytes),
            ..self
        }
    }

    pub fn with_objects_per_list(self, objects: usize) -> Self {
        Self {
            objects_per_list: objects,
            ..self
        }
    }

    /// How long a read-write transaction lives.
    pub fn read_write(&self) -> Duration {
        self.read_write
    }

    /// How long a read-only transaction lives.
    pub fn read_only(&self) -> Duration {
        self.read_only
    }

    /// The largest write-set record, in bytes.
    pub fn max_write_set(&self) -> usize {
        self.max_write_set
    }

    /// The largest read set, in bytes as [`crate::ReadSet::bytes`] counts them.
    pub fn max_read_set(&self) -> usize {
        self.max_read_set
    }

    /// The most objects of one list a write set names one by one; a list changed by more is named whole.
    pub fn objects_per_list(&self) -> usize {
        self.objects_per_list
    }
}
