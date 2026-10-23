/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, codec/codec.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// Where a [`Decoder`](crate::Decoder) seek stops relative to its target: Go's `SeekStart` and `SeekCurrent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SeekPos {
    /// Searches relative to the UID, returning it in the results.
    Start,
    /// Searches with the UID as an offset, not as part of the results.
    Current,
}

impl SeekPos {
    /// Whether `value` lies at or past `target` for this position: at or after it for `Start`, after it for
    /// `Current`.
    pub(crate) fn reached(self, value: u64, target: u64) -> bool {
        match self {
            SeekPos::Start => value >= target,
            SeekPos::Current => value > target,
        }
    }
}
