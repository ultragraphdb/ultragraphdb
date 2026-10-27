/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A fault the simulated storage injects into one operation, as Apache Iggy's simulator does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FaultMode {
    /// The operation fails with an error and has no effect.
    Before,
    /// The operation takes effect, then fails with an error.
    After,
    /// A write stores only a prefix of its bytes and fails with an error; any other operation takes effect and
    /// fails, as [`FaultMode::After`]. A torn write past the end grows the file to the write's end, as the file system
    /// records it, with zeros where the drive stored nothing.
    TornWrite,
    /// A write stores only a prefix of its bytes and reports success, as a drive that tears a write and tells nobody;
    /// any other operation is not faulted. The file grows as for [`FaultMode::TornWrite`].
    SilentTornWrite,
    /// A write stores only a prefix of its bytes, the file grows only as far as that prefix, and the write fails with
    /// an error: a crash during an unsynced write that extends the file, whose whole length the file system never
    /// recorded. Any other operation takes effect and fails, as [`FaultMode::After`]. A random fault draws it in place
    /// of [`FaultMode::TornWrite`] half the time.
    ShortTornWrite,
}
