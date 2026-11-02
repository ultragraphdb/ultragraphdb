/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU32;

use ultragraphdb_error_cluster::ClusterError;

/// The sequencer's settings: the epoch lengths, required with no default, and the bounds of what it holds in memory.
///
/// `epoch_ms_min` lies from 1 to 1,000: from 1 ms, 42-bit epoch numbers last over a century, and up to 1 s, a waiting
/// read or a full partition waits at most that long for its requested close. `epoch_ms_max` lies from `epoch_ms_min`
/// to 10,000: up to 10 s, a snapshot never lags the latest commit by more than the read-write transaction limit. The
/// sequencer holds the last `closes_retained` closes, for subscribers catching up and for mapping a time to an epoch,
/// and at most `transactions_retained` multi-partition transactions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SequencerConfig {
    epoch_ms_min: u64,
    epoch_ms_max: u64,
    closes_retained: NonZeroU32,
    transactions_retained: NonZeroU32,
}

impl SequencerConfig {
    /// The largest `epoch_ms_min`, 1,000.
    pub const EPOCH_MS_MIN_MAX: u64 = 1_000;

    /// The largest `epoch_ms_max`, 10,000.
    pub const EPOCH_MS_MAX_MAX: u64 = 10_000;

    /// The closes held unless set otherwise, 65,536.
    pub const CLOSES_RETAINED: NonZeroU32 = NonZeroU32::MIN.saturating_add(65_535);

    /// The multi-partition transactions held unless set otherwise, 65,536.
    pub const TRANSACTIONS_RETAINED: NonZeroU32 = NonZeroU32::MIN.saturating_add(65_535);

    /// The settings with epochs closing at least `epoch_ms_min` and at most `epoch_ms_max` milliseconds apart.
    ///
    /// # Errors
    ///
    /// [`ClusterError::setting_below`] when `epoch_ms_min` is 0 or `epoch_ms_max` is below `epoch_ms_min`, and
    /// [`ClusterError::setting_above`] when `epoch_ms_min` is above 1,000 or `epoch_ms_max` above 10,000, each naming the
    /// setting and the bound; `epoch_ms_min` is checked first.
    pub fn new(epoch_ms_min: u64, epoch_ms_max: u64) -> Result<Self, ClusterError> {
        if epoch_ms_min == 0 {
            return Err(ClusterError::setting_below(
                "epoch_ms_min",
                0,
                "its minimum",
                1,
            ));
        }
        if epoch_ms_min > Self::EPOCH_MS_MIN_MAX {
            return Err(ClusterError::setting_above(
                "epoch_ms_min",
                epoch_ms_min,
                Self::EPOCH_MS_MIN_MAX,
            ));
        }
        if epoch_ms_max < epoch_ms_min {
            return Err(ClusterError::setting_below(
                "epoch_ms_max",
                epoch_ms_max,
                "epoch_ms_min",
                epoch_ms_min,
            ));
        }
        if epoch_ms_max > Self::EPOCH_MS_MAX_MAX {
            return Err(ClusterError::setting_above(
                "epoch_ms_max",
                epoch_ms_max,
                Self::EPOCH_MS_MAX_MAX,
            ));
        }
        Ok(Self {
            epoch_ms_min,
            epoch_ms_max,
            closes_retained: Self::CLOSES_RETAINED,
            transactions_retained: Self::TRANSACTIONS_RETAINED,
        })
    }

    /// The settings holding the last `closes` closes.
    #[must_use]
    pub fn with_closes_retained(self, closes: NonZeroU32) -> Self {
        Self {
            closes_retained: closes,
            ..self
        }
    }

    /// The settings holding at most `transactions` multi-partition transactions.
    #[must_use]
    pub fn with_transactions_retained(self, transactions: NonZeroU32) -> Self {
        Self {
            transactions_retained: transactions,
            ..self
        }
    }

    /// The least time between two closes, in milliseconds.
    #[must_use]
    pub fn epoch_ms_min(&self) -> u64 {
        self.epoch_ms_min
    }

    /// The most time between two closes, in milliseconds.
    #[must_use]
    pub fn epoch_ms_max(&self) -> u64 {
        self.epoch_ms_max
    }

    /// The closes held.
    #[must_use]
    pub fn closes_retained(&self) -> NonZeroU32 {
        self.closes_retained
    }

    /// The most multi-partition transactions held.
    #[must_use]
    pub fn transactions_retained(&self) -> NonZeroU32 {
        self.transactions_retained
    }
}
