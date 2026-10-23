/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What the simulator asks a workload to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Event {
    /// One step of the actor with this number: the apply loop, or one of its readers.
    Step(usize),
    /// The store was reopened after a crash, a power loss or a failed batch: everything the workload held in memory
    /// is gone, and its open snapshots are of the earlier process.
    Recovered,
}
