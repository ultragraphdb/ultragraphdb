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
}
