/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::Cell;
use std::rc::Rc;

use crate::types::log_io_totals::LogIoTotals;

/// The running totals a [`LogIo`] shares.
#[derive(Debug, Default)]
struct Totals {
    write_us: Cell<u64>,
    writes: Cell<u64>,
    written_bytes: Cell<u64>,
    sync_us: Cell<u64>,
    syncs: Cell<u64>,
}

/// What a partition's write-ahead log spends in its I/O, counted by its [`crate::TimedStorage`] and read by its core: running
/// totals of the time in writes and syncs, the writes and syncs, and the bytes written. A clone shares the totals.
///
/// The log's storage and the core that reads the totals are on the partition's one thread, and the log is owned by the core's
/// pipeline, which lends it to no one: `Rc` lets both hold the totals, and `Cell` lets the storage add to them through the shared
/// reference its file's sync takes. Nothing here is `Send`, so a clone cannot leave that thread.
#[derive(Debug, Clone, Default)]
pub struct LogIo {
    totals: Rc<Totals>,
}

impl LogIo {
    /// A write of `bytes` bytes took `us` microseconds.
    pub fn wrote(&self, bytes: u64, us: u64) {
        add(&self.totals.write_us, us);
        add(&self.totals.writes, 1);
        add(&self.totals.written_bytes, bytes);
    }

    /// A sync took `us` microseconds.
    pub fn synced(&self, us: u64) {
        add(&self.totals.sync_us, us);
        add(&self.totals.syncs, 1);
    }

    /// The totals now.
    #[must_use]
    pub fn totals(&self) -> LogIoTotals {
        let totals = &self.totals;
        LogIoTotals::new(
            (
                totals.write_us.get(),
                totals.writes.get(),
                totals.written_bytes.get(),
            ),
            (totals.sync_us.get(), totals.syncs.get()),
        )
    }
}

/// Adds `amount` to `total`, which stays at `u64::MAX` past it.
fn add(total: &Cell<u64>, amount: u64) {
    total.set(total.get().saturating_add(amount));
}
