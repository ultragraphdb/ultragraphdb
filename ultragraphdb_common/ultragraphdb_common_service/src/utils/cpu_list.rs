/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

/// The most CPUs a list may name, past any machine a role runs on, so that a hostile list cannot allocate without bound.
const CPUS_MAX: usize = 1 << 16;

/// The CPUs of a kernel CPU list such as `0-3,8`; `None` for one that is malformed or names too many.
#[must_use]
pub fn parse_cpu_list(list: &str) -> Option<BTreeSet<usize>> {
    let mut cpus = BTreeSet::new();
    for part in list.trim().split(',') {
        let (first, last) = part.split_once('-').unwrap_or((part, part));
        let (first, last): (usize, usize) = (first.parse().ok()?, last.parse().ok()?);
        if first > last || last >= CPUS_MAX {
            return None;
        }
        // The kernel's `first-last` names both ends, and `last` is below `CPUS_MAX`, so the closed range is the list's own.
        cpus.extend(first..=last);
    }
    Some(cpus)
}

/// The CPUs a `/proc/*/status` text allows, from its `Cpus_allowed_list` line.
#[must_use]
pub fn allowed_in_status(status: &str) -> Option<BTreeSet<usize>> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("Cpus_allowed_list:"))
        .and_then(parse_cpu_list)
}
