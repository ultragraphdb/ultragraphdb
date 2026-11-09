/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io;

use ultragraphdb_transactions_log::{AlignedBuf, DurableFile};

use crate::types::timed_file::TimedFile;
use crate::utils::micros_since::micros_since;

/// Each future is `inner`'s, so a future dropped at its wait leaves what `inner`'s leaves and counts nothing.
impl<F: DurableFile> DurableFile for TimedFile<F> {
    async fn read_at(&self, offset: u64, len: usize) -> io::Result<AlignedBuf> {
        self.inner.read_at(offset, len).await
    }

    /// Counted once it completes, failed or not, with the bytes it was given.
    async fn write_at(&mut self, offset: u64, bytes: AlignedBuf) -> io::Result<()> {
        // A `usize` is at most 64 bits on every target the repository builds for, so the length converts losslessly.
        let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        let started = micros_since(self.origin);
        let written = self.inner.write_at(offset, bytes).await;
        self.io
            .wrote(len, micros_since(self.origin).saturating_sub(started));
        written
    }

    async fn preallocate(&mut self, len: u64) -> io::Result<()> {
        self.inner.preallocate(len).await
    }

    async fn length(&self) -> io::Result<u64> {
        self.inner.length().await
    }

    async fn truncate(&mut self, len: u64) -> io::Result<()> {
        self.inner.truncate(len).await
    }

    /// Counted once it completes, failed or not.
    async fn sync(&self) -> io::Result<()> {
        let started = micros_since(self.origin);
        let synced = self.inner.sync().await;
        self.io
            .synced(micros_since(self.origin).saturating_sub(started));
        synced
    }
}
