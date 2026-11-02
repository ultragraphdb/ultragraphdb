/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::Epoch;
use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_traits_log::OrderedLog;

use crate::types::close::Close;
use crate::types::sequencer::Sequencer;

impl<L: OrderedLog> Sequencer<L> {
    /// The latest epoch whose durable close was recorded at or before `time_us`.
    ///
    /// # Errors
    ///
    /// [`ClusterError::before_first_close`] naming `time_us` and the oldest close held when `time_us` lies before it,
    /// or no epoch has closed.
    pub fn as_of(&self, time_us: u64) -> Result<Epoch, ClusterError> {
        // The durable closes are a prefix of those held, and their times never fall, so those at or before
        // `time_us` are a prefix too.
        let at_or_before = self
            .closes
            .partition_point(|close| close.epoch() <= self.closed && close.time_us() <= time_us);
        match at_or_before
            .checked_sub(1)
            .and_then(|index| self.closes.get(index))
        {
            Some(close) => Ok(close.epoch()),
            None => {
                let first = self
                    .closes
                    .front()
                    .filter(|close| close.epoch() <= self.closed);
                Err(ClusterError::before_first_close(
                    time_us,
                    first.map(Close::time_us),
                ))
            }
        }
    }
}
