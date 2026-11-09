/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Instant;

/// The microseconds since `origin`, `u64::MAX` past 584,942 years: the one clock the storage role's shell reads, for its
/// partitions' hosts, which read none, and for timing a commit's phases and its log's I/O.
#[expect(
    clippy::disallowed_methods,
    reason = "the shell reads the clock: it tells the partition's host, which reads none, and times the core's phases"
)]
pub(crate) fn micros_since(origin: Instant) -> u64 {
    u64::try_from(origin.elapsed().as_micros()).unwrap_or(u64::MAX)
}
