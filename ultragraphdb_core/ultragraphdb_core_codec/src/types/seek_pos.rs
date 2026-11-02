/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// Where a [`Decoder`](crate::Decoder) seek stops relative to its target: Go's `SeekStart` and `SeekCurrent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SeekPos {
    /// Searches relative to the target, returning it in the results.
    Start,
    /// Searches with the target as an offset, not as part of the results.
    Current,
}

impl SeekPos {
    /// Whether `value` lies at or past `target` for this position: at or after it for `Start`, after it for
    /// `Current`.
    pub(crate) fn reached(self, value: u128, target: u128) -> bool {
        match self {
            SeekPos::Start => value >= target,
            SeekPos::Current => value > target,
        }
    }
}
