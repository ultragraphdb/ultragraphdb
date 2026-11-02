/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU64;

use crate::errors::id_error::IdError;

/// The 64-bit sequence of a [`SystemId`](crate::SystemId), unique within its type across the cluster and never
/// reused. Sequence 0 is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sequence(NonZeroU64);

impl Sequence {
    /// The sequence `sequence`.
    ///
    /// # Errors
    ///
    /// [`IdError::zero_sequence`] when `sequence` is 0.
    pub fn new(sequence: u64) -> Result<Self, IdError> {
        NonZeroU64::new(sequence)
            .map(Self)
            .ok_or_else(IdError::zero_sequence)
    }

    /// The sequence as a number.
    #[must_use]
    pub fn get(self) -> u64 {
        self.0.get()
    }
}
