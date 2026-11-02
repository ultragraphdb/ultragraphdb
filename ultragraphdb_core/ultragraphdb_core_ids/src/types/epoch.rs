/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::id_error::IdError;

/// An epoch, the upper 42 bits of a [`Version`](crate::Version). The sequencer closes epochs in order from 1; a
/// partition stamps its commits with its open epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Epoch(u64);

impl Epoch {
    /// The largest epoch, 2^42 − 1.
    pub const MAX: Self = Self((1 << 42) - 1);

    /// The epoch `epoch`.
    ///
    /// # Errors
    ///
    /// [`IdError::epoch_range`] when `epoch` is above [`Epoch::MAX`].
    pub fn new(epoch: u64) -> Result<Self, IdError> {
        if epoch <= Self::MAX.0 {
            Ok(Self(epoch))
        } else {
            Err(IdError::epoch_range(epoch))
        }
    }

    /// The epoch as a number.
    #[must_use]
    pub fn get(self) -> u64 {
        self.0
    }

    /// The epoch after this one.
    ///
    /// # Errors
    ///
    /// [`IdError::epoch_range`] naming the epoch past the last when this is [`Epoch::MAX`].
    pub fn next(self) -> Result<Self, IdError> {
        // At most 2^42 − 1, so the sum is at most 2^42 and cannot overflow.
        Self::new(self.0.saturating_add(1))
    }

    /// The epoch of `value`'s low 42 bits: a version's upper bits, shifted down, which hold 42 bits by construction.
    pub(crate) fn from_low_bits(value: u64) -> Self {
        debug_assert!(value <= Self::MAX.0, "a version's epoch holds 42 bits");
        Self(value & Self::MAX.0)
    }
}
