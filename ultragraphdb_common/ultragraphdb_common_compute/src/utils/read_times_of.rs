/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use crate::types::compute_core::ReadTimes;

/// `scan_stall_ms` when compute's settings name none, 30,000 ms (`PROTOCOL.md` §11.9).
const SCAN_STALL_MS: u64 = 30_000;

/// `read_retain_ms` when compute's settings name none, 60,000 ms (`PROTOCOL.md` §11.9).
const READ_RETAIN_MS: u64 = 60_000;

/// The times a read keeps its versions by, from the `[storage]` settings compute holds, each by its default when absent.
#[must_use]
pub fn read_times_of(scan_stall_ms: Option<u64>, read_retain_ms: Option<u64>) -> ReadTimes {
    ReadTimes::new(
        Duration::from_millis(scan_stall_ms.unwrap_or(SCAN_STALL_MS)),
        Duration::from_millis(read_retain_ms.unwrap_or(READ_RETAIN_MS)),
    )
}
