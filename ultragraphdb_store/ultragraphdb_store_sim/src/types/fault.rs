/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::drop_stage::DropStage;
use crate::types::rebuild_stage::RebuildStage;

/// A fault forced on the next operation it applies to, besides those the fault stream draws: what a test injects
/// at a chosen point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Fault {
    /// The next batch fails during its persist and poisons the store.
    FailNextBatch,
    /// The process crashes while the next batch is written.
    CrashDuringNextBatch,
    /// The process crashes when the next drop reaches the stage.
    CrashDuringNextDrop(DropStage),
    /// The power fails when the next drop reaches the stage.
    PowerLossDuringNextDrop(DropStage),
    /// The process crashes when the next rebuild reaches the stage.
    CrashDuringNextRebuild(RebuildStage),
    /// The power fails when the next rebuild reaches the stage.
    PowerLossDuringNextRebuild(RebuildStage),
    /// The next point read fails.
    FailNextRead,
    /// The next scan fails after yielding this many items.
    FailNextScanAfter(usize),
}
