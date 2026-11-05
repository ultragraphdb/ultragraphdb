/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a walk's visit made of one key ([`crate::walk_lists`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalkStep {
    /// The key gave no row, and the walk goes on.
    Nothing,
    /// The key gave a row, and the walk goes on.
    Row,
    /// The key gave part of its row, and the walk stops inside it.
    PausedInside,
    /// The key gave nothing, and the walk stops before it.
    PausedBefore,
}
