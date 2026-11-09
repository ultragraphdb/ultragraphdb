/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod timed_storage_durable_storage;

use std::time::Instant;

use crate::types::log_io::LogIo;

/// A partition's write-ahead log storage, `inner`, its writes and syncs timed into a [`LogIo`], so its core can say how much of
/// a commit the log's I/O took. Every operation is `inner`'s; a file it opens is a [`crate::TimedFile`], and a directory's sync
/// counts as a sync.
#[derive(Debug, Clone)]
pub struct TimedStorage<S> {
    inner: S,
    io: LogIo,
    origin: Instant,
}

impl<S> TimedStorage<S> {
    /// `inner`, its I/O counted into `io` on the clock that began at `origin`.
    #[must_use]
    pub fn new(inner: S, io: LogIo, origin: Instant) -> Self {
        Self { inner, io, origin }
    }

    /// Where its I/O is counted.
    #[must_use]
    pub fn io(&self) -> &LogIo {
        &self.io
    }
}
