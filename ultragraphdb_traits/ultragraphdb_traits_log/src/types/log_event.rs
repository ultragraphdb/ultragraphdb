/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::position::Position;

/// What a replicated log tells the sequencer outside an append's result. The local write-ahead log has neither: one
/// node has no views, and it never replaces a round it accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogEvent {
    /// The replicas moved to `view`; this node's role in it follows from the view.
    ViewChanged { view: u32 },
    /// The uncommitted rounds from `from` on were replaced by the new view's: their appends' outcomes are unknown.
    SuffixReplaced { from: Position },
}
