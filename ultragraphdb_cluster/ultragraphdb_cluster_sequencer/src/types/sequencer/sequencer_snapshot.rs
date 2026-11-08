/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_traits_log::{OrderedLog, Position};

use crate::types::close::Close;
use crate::types::sequenced_transaction::SequencedTransaction;
use crate::types::sequencer::Sequencer;
use crate::types::sequencer_snapshot::SequencerSnapshot;

impl<L: OrderedLog> Sequencer<L> {
    /// The durable state as of the last durable position, as a snapshot for the host to store: the durable closes
    /// retained and the durable multi-partition transactions held.
    #[must_use]
    pub fn snapshot(&self) -> SequencerSnapshot {
        let closes: Vec<Close> = self
            .closes
            .iter()
            .take_while(|close| close.epoch() <= self.closed)
            .cloned()
            .collect();
        let transactions: Vec<(Position, SequencedTransaction)> = self
            .transactions
            .iter()
            .take_while(|(position, _)| *position <= self.durable)
            .cloned()
            .collect();
        SequencerSnapshot::from_held(self.durable, closes, transactions)
    }

    /// Trims the log through `snapshot`'s position, which the host has stored durably, so the next recovery starts
    /// from it ([`Sequencer::recover_from`]); a snapshot at or below the log's checkpoint changes nothing. The log of
    /// a sequencer closing 100 epochs a second fills its 4 GiB in about three hours unless trimmed.
    ///
    /// # Errors
    ///
    /// [`ClusterError::log`] when the log refuses the trim: a position past its committed one, or a failure, which
    /// poisons it.
    ///
    /// A future dropped at the log's trim leaves the sequencer unchanged and the log's checkpoint at the snapshot's
    /// position or still before it; a later `trim` with the same snapshot is safe either way, since a trim at or
    /// below the checkpoint changes nothing.
    pub async fn trim(&mut self, snapshot: &SequencerSnapshot) -> Result<(), ClusterError> {
        self.log.trim_through(snapshot.position()).await?;
        Ok(())
    }
}
