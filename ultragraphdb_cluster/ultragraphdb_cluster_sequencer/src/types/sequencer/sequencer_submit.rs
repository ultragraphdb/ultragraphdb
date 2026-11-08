/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_traits_log::{OrderedLog, Position};

use crate::types::sequenced_transaction::SequencedTransaction;
use crate::types::sequencer::Sequencer;
use crate::types::sequencer_record::SequencerRecord;

impl<L: OrderedLog> Sequencer<L> {
    /// Accepts a multi-partition transaction naming `partitions`, the largest reply epoch `reply_epoch` among its
    /// registrations and its `body`, and places it in the open epoch, after every transaction accepted before it;
    /// returns the epoch and the log position it was appended at. It is acknowledged once [`sync`](Self::sync) reports
    /// a durable position at or past it.
    ///
    /// # Errors
    ///
    /// The refusals of [`SequencedTransaction::new`] (no partition, too many, a partition named twice, a body too
    /// large, `reply_epoch` past the open epoch); [`ClusterError::epoch_limit`] when no epoch is open after epoch
    /// 2^42 − 1; [`ClusterError::full`] when the sequencer holds `transactions_retained` transactions;
    /// [`ClusterError::log`] when the log refuses the round. Nothing is appended on a refusal.
    pub fn submit(
        &mut self,
        reply_epoch: Epoch,
        partitions: &[PartitionId],
        body: &[u8],
    ) -> Result<(Epoch, Position), ClusterError> {
        let last = self.last_appended();
        let epoch = Epoch::new(last + 1).map_err(|_| ClusterError::epoch_limit(last))?;
        // The setting is a `u32`, which a `usize` of 32 bits or more holds whole on every target the node builds for.
        let capacity = self.config.transactions_retained().get() as usize;
        if self.transactions.len() >= capacity {
            return Err(ClusterError::full("multi-partition transactions", capacity));
        }
        let transaction =
            SequencedTransaction::new(epoch, reply_epoch, partitions.to_vec(), body.to_vec())?;
        let position = self.append(SequencerRecord::Transaction(transaction))?;
        Ok((epoch, position))
    }

    /// The durable multi-partition transactions placed in `epoch` or later, in their order.
    #[must_use]
    pub fn transactions_from(&self, epoch: Epoch) -> Vec<&SequencedTransaction> {
        self.transactions
            .iter()
            .take_while(|(position, _)| *position <= self.durable)
            .map(|(_, transaction)| transaction)
            .filter(|transaction| transaction.epoch() >= epoch)
            .collect()
    }
}
