/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// How the store carries durability until a Raft log does: Dgraph's `--survive` modes (`x/flags.go:41-47`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Durability {
    /// `process`: each batch is handed to the operating system, which keeps it through a crash of the process, and
    /// the store syncs once a minute, as Dgraph's `x.StoreSync` does (`x/x.go:1281-1297`).
    #[default]
    Process,
    /// `filesystem`: each batch is synced before it is acknowledged, as Dgraph's `HardSync` syncs each commit
    /// (`worker/draft.go:1005-1009`), so it survives a power loss.
    Filesystem,
}
