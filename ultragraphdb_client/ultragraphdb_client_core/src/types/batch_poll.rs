/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What reading a result stream's next batch gave.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BatchPoll<T> {
    /// The batch, read in place by the caller's function, which returned this.
    Batch(T),
    /// The stream has given its last batch.
    Done,
    /// No batch has arrived yet: the shell reads more frames, and sends what the client queued (a Credit).
    Wait,
}
