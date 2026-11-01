/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! EXPERIMENT (2026-11-01): store operations counted per run, for the write-path investigation.

use std::sync::atomic::{AtomicU64, Ordering};

pub static VERSIONLESS_GETS: AtomicU64 = AtomicU64::new(0);
pub static VERSIONLESS_MISSES: AtomicU64 = AtomicU64::new(0);
pub static SNAPSHOT_GETS: AtomicU64 = AtomicU64::new(0);
pub static SNAPSHOT_VERSIONS: AtomicU64 = AtomicU64::new(0);
pub static COMMIT_APPLIES: AtomicU64 = AtomicU64::new(0);
pub static COMMIT_ENTRIES: AtomicU64 = AtomicU64::new(0);
pub static OTHER_ENTRIES: AtomicU64 = AtomicU64::new(0);

pub fn add(counter: &AtomicU64, n: u64) {
    counter.fetch_add(n, Ordering::Relaxed);
}

/// The counts as JSON, per relation over `relations`.
pub fn json(relations: u64) -> String {
    let r = relations.max(1) as f64;
    let g = |c: &AtomicU64| c.load(Ordering::Relaxed);
    format!(
        "{{\"relations\":{relations},\"versionless_gets_per_rel\":{:.2},\"versionless_misses_per_rel\":{:.2},\"snapshot_gets_per_rel\":{:.2},\"snapshot_version_scans_per_rel\":{:.2},\"commit_applies\":{},\"commit_entries_per_rel\":{:.2},\"other_entries_per_rel\":{:.2}}}",
        g(&VERSIONLESS_GETS) as f64 / r,
        g(&VERSIONLESS_MISSES) as f64 / r,
        g(&SNAPSHOT_GETS) as f64 / r,
        g(&SNAPSHOT_VERSIONS) as f64 / r,
        g(&COMMIT_APPLIES),
        g(&COMMIT_ENTRIES) as f64 / r,
        g(&OTHER_ENTRIES) as f64 / r,
    )
}
