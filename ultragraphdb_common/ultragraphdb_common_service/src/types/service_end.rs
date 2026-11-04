/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// How a role's service ended without failing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceEnd {
    /// A signal stopped it.
    Stopped,
    /// An administrator asked it to load this revision, by executing itself again.
    Load(u64),
}
