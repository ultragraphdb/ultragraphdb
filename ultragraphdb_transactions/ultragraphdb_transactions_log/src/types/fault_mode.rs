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
    /// fails, as [`FaultMode::After`].
    TornWrite,
    /// A write stores only a prefix of its bytes and reports success, as a drive that tears a write and tells nobody;
    /// any other operation is not faulted.
    SilentTornWrite,
}
