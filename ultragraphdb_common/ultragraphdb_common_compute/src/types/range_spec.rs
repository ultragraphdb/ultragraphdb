/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::utils::keys::past_alone;

/// One range a read has yet to Scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RangeSpec {
    pub(crate) first: Vec<u8>,
    pub(crate) past: Vec<u8>,
    pub(crate) after: Vec<u8>,
}

impl RangeSpec {
    /// The range holding `key` alone, resuming its list at `after`.
    pub(crate) fn alone(key: Vec<u8>, after: Vec<u8>) -> Self {
        let past = past_alone(&key);
        Self {
            first: key,
            past,
            after,
        }
    }

    /// Whether the range holds one key alone.
    pub(crate) fn is_alone(&self) -> bool {
        self.past.strip_prefix(self.first.as_slice()) == Some(&[0][..])
    }
}
