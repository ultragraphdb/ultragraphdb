/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod timed_file_durable_file;

use std::time::Instant;

use crate::types::log_io::LogIo;

/// A file of a [`crate::TimedStorage`]: `inner`'s, each write timed with its bytes and each sync timed into a [`LogIo`].
#[derive(Debug)]
pub struct TimedFile<F> {
    inner: F,
    io: LogIo,
    origin: Instant,
}

impl<F> TimedFile<F> {
    /// `inner`, its writes and syncs counted into `io` on the clock that began at `origin`.
    #[must_use]
    pub fn new(inner: F, io: LogIo, origin: Instant) -> Self {
        Self { inner, io, origin }
    }
}
