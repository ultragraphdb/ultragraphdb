/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a partition's write-ahead log had spent in its I/O at a moment, each a running total since the partition opened its
/// log: the time in writes and the writes, the bytes written, the time in syncs and the syncs. Control plane: a copy, made
/// once per commit taken and answered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LogIoTotals {
    write_us: u64,
    writes: u64,
    written_bytes: u64,
    sync_us: u64,
    syncs: u64,
}

impl LogIoTotals {
    /// Totals of `writes` writes of `written_bytes` bytes taking `write_us` microseconds, and of `syncs` syncs taking `sync_us`.
    #[must_use]
    pub fn new(
        (write_us, writes, written_bytes): (u64, u64, u64),
        (sync_us, syncs): (u64, u64),
    ) -> Self {
        Self {
            write_us,
            writes,
            written_bytes,
            sync_us,
            syncs,
        }
    }

    /// What was spent from `earlier` to these totals; nothing where `earlier` is past them.
    #[must_use]
    pub fn since(&self, earlier: &Self) -> Self {
        Self {
            write_us: self.write_us.saturating_sub(earlier.write_us),
            writes: self.writes.saturating_sub(earlier.writes),
            written_bytes: self.written_bytes.saturating_sub(earlier.written_bytes),
            sync_us: self.sync_us.saturating_sub(earlier.sync_us),
            syncs: self.syncs.saturating_sub(earlier.syncs),
        }
    }

    /// The microseconds spent in writes.
    #[must_use]
    pub fn write_us(&self) -> u64 {
        self.write_us
    }

    /// The writes.
    #[must_use]
    pub fn writes(&self) -> u64 {
        self.writes
    }

    /// The bytes written.
    #[must_use]
    pub fn written_bytes(&self) -> u64 {
        self.written_bytes
    }

    /// The microseconds spent in syncs, a file's `fdatasync` and a directory's sync alike.
    #[must_use]
    pub fn sync_us(&self) -> u64 {
        self.sync_us
    }

    /// The syncs.
    #[must_use]
    pub fn syncs(&self) -> u64 {
        self.syncs
    }
}
