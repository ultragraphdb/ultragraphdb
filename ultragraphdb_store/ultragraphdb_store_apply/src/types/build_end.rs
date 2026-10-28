/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// How an index build sequenced through the commit step ends, under its record's number: a proposal the step decides
/// in a round of its own, which writes at the version the step gives it (decided 2026-10-28).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuildEnd {
    /// The build is done: its lists and new declaration are swapped in.
    Swap(u64),
    /// The build failed, or a drop cancelled it: its old declaration is written back, when it is still held, and its
    /// record removed.
    Undo(u64),
}
