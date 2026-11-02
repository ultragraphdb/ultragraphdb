/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::Epoch;
use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_traits_log::OrderedLog;

use crate::types::catalog_version::CatalogVersion;
use crate::types::clock_reading::ClockReading;
use crate::types::close::Close;
use crate::types::sequencer::Sequencer;
use crate::types::sequencer_record::SequencerRecord;

impl<L: OrderedLog> Sequencer<L> {
    /// Asks for the close of `epoch` as soon as `epoch_ms_min` allows: a partition whose sequence would run out, or
    /// the frontend for a snapshot read waiting on the open epoch. A request for an epoch already closed, durable or
    /// not, is answered by that close and changes nothing.
    ///
    /// # Errors
    ///
    /// [`ClusterError::epoch_ahead`] when `epoch` is past the open epoch, one after the last close appended.
    pub fn request_close(&mut self, epoch: Epoch) -> Result<(), ClusterError> {
        // The last close appended is at most 2^42 - 1, so the open epoch's number cannot overflow.
        let open = self.last_appended() + 1;
        if epoch.get() > open {
            return Err(ClusterError::epoch_ahead(epoch.get(), open));
        }
        if epoch.get() == open {
            self.close_requested = true;
        }
        Ok(())
    }

    /// Closes the open epoch when it is due at `now` ([`deadline`](Self::deadline)), announcing `catalog`, the
    /// versions the metadata group holds pending, and carrying `evictions`, the sessions its registry evicted since
    /// the last close; returns the epoch closed, `None` when no close is due. The close is recorded at `now`'s
    /// wall-clock time, or at the last close's when the clock stepped back, and appended to the log; it is delivered
    /// once [`sync`](Self::sync) makes it durable.
    ///
    /// # Errors
    ///
    /// [`ClusterError::epoch_limit`] when the last close appended is of epoch 2^42 − 1; [`ClusterError::too_many`]
    /// when `catalog` or `evictions` is past its limit ([`Close::new`](crate::Close::new)); [`ClusterError::log`] when
    /// the log refuses the round. Nothing changes on a refusal.
    pub fn tick(
        &mut self,
        now: ClockReading,
        catalog: &[CatalogVersion],
        evictions: &[u128],
    ) -> Result<Option<Epoch>, ClusterError> {
        if now.monotonic_us() < self.deadline() {
            return Ok(None);
        }
        let last = self.last_appended();
        let epoch = Epoch::new(last + 1).map_err(|_| ClusterError::epoch_limit(last))?;
        let time_us = now.wall_us().max(self.last_time_us());
        let close = Close::new(epoch, time_us, catalog.to_vec(), evictions.to_vec())?;
        let _position = self.append(SequencerRecord::Close(close))?;
        self.last_close_us = now.monotonic_us();
        self.close_requested = false;
        Ok(Some(epoch))
    }
}
