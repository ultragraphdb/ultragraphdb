/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// How far a batch is persisted before [`crate::Store::apply`] returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PersistMode {
    /// Handed whole to the operating system, which keeps it through a crash of the process: fjall's
    /// `PersistMode::Buffer`, the least a batch is written with.
    Buffer,
    /// Synced to the disk, which keeps it through a power loss: fjall's `PersistMode::SyncAll`.
    Sync,
}
