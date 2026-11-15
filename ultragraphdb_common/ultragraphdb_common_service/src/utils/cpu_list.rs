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
        // The kernel's `first-last` names both ends, so the half-open range ends past `last`, which is below `CPUS_MAX`:
        // the end cannot overflow.
        cpus.extend(first..last + 1);
    }
    Some(cpus)
}

/// The CPUs a role's runtimes take: `allowed` with its first `skip` left free, for the CPUs a host gives its interrupts.
/// A `skip` that would leave no CPU leaves the last one; empty only for an empty `allowed`.
#[must_use]
pub fn usable_cpus(allowed: &BTreeSet<usize>, skip: usize) -> BTreeSet<usize> {
    let skipped = skip.min(allowed.len().saturating_sub(1));
    allowed.iter().skip(skipped).copied().collect()
}

/// The CPU the `core`th runtime of a role binds to: the [`usable_cpus`] of `allowed` and `skip` taken in turn, so more
/// runtimes than CPUs share them evenly. `None` only for an empty `allowed`.
#[must_use]
pub fn cpu_for(allowed: &BTreeSet<usize>, skip: usize, core: usize) -> Option<usize> {
    let usable = usable_cpus(allowed, skip);
    let turn = core.checked_rem(usable.len())?;
    usable.iter().nth(turn).copied()
}

/// The CPUs a `/proc/*/status` text allows, from its `Cpus_allowed_list` line.
#[must_use]
pub fn allowed_in_status(status: &str) -> Option<BTreeSet<usize>> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("Cpus_allowed_list:"))
        .and_then(parse_cpu_list)
}
