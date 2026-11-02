/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_traits_log::OrderedLog;

use crate::types::close::Close;
use crate::types::sequencer::Sequencer;

impl<L: OrderedLog> Sequencer<L> {
    /// The most partitions subscribed at once, the placement map's limit, 1,024.
    pub const SUBSCRIBERS_MAX: usize = 1_024;

    /// Subscribes `partition` to the closes after `after`, the last epoch delivered to it, 0 for none: a new
    /// partition, or one reconnecting, which receives the closes it missed first. A partition already subscribed is
    /// moved to `after`.
    ///
    /// # Errors
    ///
    /// [`ClusterError::epoch_ahead`] when `after` is past the last durable close; [`ClusterError::full`] when
    /// [`Sequencer::SUBSCRIBERS_MAX`] other partitions are subscribed.
    pub fn subscribe(&mut self, partition: PartitionId, after: Epoch) -> Result<(), ClusterError> {
        if after > self.closed {
            return Err(ClusterError::epoch_ahead(after.get(), self.closed.get()));
        }
        if !self.subscribers.contains_key(&partition)
            && self.subscribers.len() >= Self::SUBSCRIBERS_MAX
        {
            return Err(ClusterError::full("subscribers", Self::SUBSCRIBERS_MAX));
        }
        let _moved: Option<Epoch> = self.subscribers.insert(partition, after);
        Ok(())
    }

    /// Ends the subscription of `partition`, when it has one.
    pub fn unsubscribe(&mut self, partition: PartitionId) {
        let _ended: Option<Epoch> = self.subscribers.remove(&partition);
    }

    /// The next durable close not yet delivered to `partition`, in epoch order, counted as delivered; `None` when every
    /// durable close has been.
    ///
    /// # Errors
    ///
    /// [`ClusterError::not_subscribed`] when `partition` has no subscription; [`ClusterError::catch_up_trimmed`] when
    /// the next close it needs is older than the oldest close held.
    pub fn take_delivery(&mut self, partition: PartitionId) -> Result<Option<Close>, ClusterError> {
        let after = *self
            .subscribers
            .get(&partition)
            .ok_or_else(|| ClusterError::not_subscribed(partition.get()))?;
        // At most the last durable close, so one more cannot overflow.
        let next = after.get() + 1;
        if next > self.closed.get() {
            return Ok(None);
        }
        let oldest = self
            .closes
            .front()
            .map_or(next, |close| close.epoch().get());
        if next < oldest {
            return Err(ClusterError::catch_up_trimmed(after.get(), oldest));
        }
        // The closes held are consecutive from the oldest, so the next lies `next - oldest` after it.
        let index = usize::try_from(next - oldest).unwrap_or(usize::MAX);
        let close = self.closes.get(index).cloned();
        debug_assert!(close.as_ref().is_some_and(|c| c.epoch().get() == next));
        let delivered = close.as_ref().map_or(after, Close::epoch);
        let _previous: Option<Epoch> = self.subscribers.insert(partition, delivered);
        Ok(close)
    }
}
