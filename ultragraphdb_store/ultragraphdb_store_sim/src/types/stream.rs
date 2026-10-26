/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The streams a [`crate::Seed`] derives, one per source of choice in a run, so that drawing more from one stream
/// leaves the others unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Stream {
    /// The operations a workload performs.
    Workload,
    /// The faults the simulated store injects into its operations.
    Faults,
    /// The crashes and power losses the simulator injects between steps.
    Process,
    /// The order in which the simulator steps the apply loop, its readers and the executor.
    Scheduler,
    /// The entries the simulated cache evicts.
    Cache,
    /// The order the stepped executor runs its tasks in.
    Executor,
    /// The earlier reads a workload's readers make again through their open snapshots.
    Rereads,
    /// The writes and reads a workload makes in the keyspace without versions.
    Versionless,
}
