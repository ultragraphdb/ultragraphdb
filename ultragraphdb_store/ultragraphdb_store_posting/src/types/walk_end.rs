/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// How a walk of a range of lists ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalkEnd {
    /// The range holds no more keys.
    Exhausted,
    /// The walk gave its limit of rows, and more keys follow.
    Stopped {
        /// The key of the last row it gave.
        last: Vec<u8>,
        /// The first key it did not read.
        next: Vec<u8>,
    },
    /// The visit stopped the walk.
    Paused {
        /// The last key that gave a row or part of one, which is the key paused inside when there is one; `None` when the
        /// walk gave nothing.
        last: Option<Vec<u8>>,
        /// The key the visit stopped at.
        next: Vec<u8>,
    },
    /// The walk read its bound of keys giving nothing, which it has covered, and hands back.
    Yielded {
        /// The key just past the last it read, where it goes on.
        next: Vec<u8>,
    },
}

impl WalkEnd {
    /// The key just past what the walk covered, where a stopped, paused or yielded walk's span ends: its last key followed by a
    /// zero byte, or where a yielded walk goes on. `None` when the range is exhausted or the walk gave nothing.
    pub fn past_last(&self) -> Option<Vec<u8>> {
        match self {
            Self::Exhausted | Self::Paused { last: None, .. } => None,
            Self::Yielded { next } => Some(next.clone()),
            Self::Stopped { last, .. }
            | Self::Paused {
                last: Some(last), ..
            } => {
                let mut past = Vec::with_capacity(last.len() + 1);
                past.extend_from_slice(last);
                past.push(0);
                Some(past)
            }
        }
    }
}
