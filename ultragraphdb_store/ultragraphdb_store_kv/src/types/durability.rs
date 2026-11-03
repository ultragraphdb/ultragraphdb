/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// How the store carries durability: beside the write-ahead log, which a partition's store runs with ([`Log`]), or on
/// its own, as Dgraph's `--survive` modes do (`x/flags.go:41-47`).
///
/// [`Log`]: Durability::Log
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Durability {
    /// `process`: each batch is handed to the operating system, which keeps it through a crash of the process, and
    /// the store syncs once a minute, as Dgraph's `x.StoreSync` does (`x/x.go:1281-1297`).
    #[default]
    Process,
    /// `filesystem`: each batch is synced before it is acknowledged, as Dgraph's `HardSync` syncs each commit
    /// (`worker/draft.go:1005-1009`), so it survives a power loss.
    Filesystem,
    /// The write-ahead log is the only stream synced per round: fjall's journal is persisted manually, so a batch
    /// written with [`ultragraphdb_traits_store::PersistMode::Buffer`] stays in fjall's journal buffer, and the
    /// store is synced only when asked, by [`ultragraphdb_traits_store::Store::sync`] at a checkpoint or a batch
    /// written with [`ultragraphdb_traits_store::PersistMode::Sync`]. After a crash the store holds a prefix of its
    /// batches, each whole, and recovery replays the log above the applied position the last of them recorded.
    Log,
}
