/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// Where a Scan stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScanState {
    /// Waiting for the close of its epoch, until `deadline_us`.
    Epoch { deadline_us: u64 },
    /// Waiting for its transaction's references' answers.
    References,
    /// Its version chosen, waiting for a turn to build its next frame.
    Queued,
    /// Its last frame handed to its connection, or gone with no credit for its next, at `since_us`; its next built once that one
    /// has left and credit allows it.
    Parked { since_us: u64 },
}
