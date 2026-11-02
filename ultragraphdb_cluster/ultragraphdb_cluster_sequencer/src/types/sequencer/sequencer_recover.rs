/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, VecDeque};

use ultragraphdb_core_ids::Epoch;
use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_traits_log::{OrderedLog, Position, Round};

use crate::types::clock_reading::ClockReading;
use crate::types::sequencer::{Sequencer, epoch_of};
use crate::types::sequencer_config::SequencerConfig;
use crate::types::sequencer_record::SequencerRecord;

/// The bytes of rounds one read of the log returns at most, 1 MiB: a round's frame at its largest.
const READ_BYTES: u32 = 1 << 20;

impl<L: OrderedLog> Sequencer<L> {
    /// The sequencer over `log`, its state rebuilt by replaying every durable round: the last closed epoch and its
    /// time, the last closes and the multi-partition transactions in their order. An empty log starts a fresh
    /// sequencer, before epoch 1. The next close is due `epoch_ms_max` after `now`, the time of the start.
    ///
    /// A log begins with its first round at position 1. Its first close may close any epoch, since a log whose
    /// earlier rounds were trimmed begins after them; from the first record on, every close closes the epoch after
    /// the last and every transaction lies in it.
    ///
    /// # Errors
    ///
    /// [`ClusterError::log`] when the log cannot be read; [`ClusterError::malformed`] when a round's body is no
    /// record; [`ClusterError::replay_diverged`] when a round contradicts the ones before it (a close not one after the
    /// last, a close time below the last, a transaction not in the epoch after the last close, a round's version not
    /// its record's epoch); [`ClusterError::full`] when the log holds more transactions than the settings retain.
    pub async fn recover(
        log: L,
        config: SequencerConfig,
        now: ClockReading,
    ) -> Result<Self, ClusterError> {
        let mut sequencer = Self {
            log,
            config,
            closes: VecDeque::new(),
            transactions: VecDeque::new(),
            durable: Position::ZERO,
            closed: Epoch::ZERO,
            last_close_us: now.monotonic_us(),
            close_requested: false,
            subscribers: BTreeMap::new(),
            buffer: Vec::new(),
        };
        sequencer.replay().await?;
        Ok(sequencer)
    }

    /// Replays every durable round in order, then counts every record held as durable.
    async fn replay(&mut self) -> Result<(), ClusterError> {
        let committed = self.log.committed();
        let mut next = Position::new(1);
        // Each read returns at least one round while any is left, so the rounds bound the reads.
        for _ in 0..committed.get() {
            if next > committed {
                break;
            }
            let rounds = self.log.read(next, READ_BYTES).await?;
            for round in &rounds {
                self.replay_round(round)?;
                next = Position::new(round.position().get().saturating_add(1));
            }
        }
        self.durable = committed;
        self.closed = epoch_of(self.last_appended());
        self.trim_closes();
        Ok(())
    }

    /// Checks `round` against the rounds before it and holds its record.
    fn replay_round(&mut self, round: &Round) -> Result<(), ClusterError> {
        let record = SequencerRecord::decode(round.body())?;
        let position = round.position().get();
        if round.first_version() != record.epoch().get() {
            return Err(ClusterError::replay_diverged(
                position,
                "a round's version equal to its record's epoch",
            ));
        }
        // The open epoch: one after the last close replayed, or after epoch 0 before the first.
        let open = self.last_appended() + 1;
        let first = self.closes.is_empty() && self.transactions.is_empty();
        match &record {
            SequencerRecord::Close(close) => {
                let epoch = close.epoch().get();
                if epoch == 0 || (!first && epoch != open) {
                    return Err(ClusterError::replay_diverged(
                        position,
                        "a close one after the last",
                    ));
                }
                if close.time_us() < self.last_time_us() {
                    return Err(ClusterError::replay_diverged(
                        position,
                        "a close time at or above the last",
                    ));
                }
            }
            SequencerRecord::Transaction(transaction) => {
                if transaction.epoch().get() != open {
                    return Err(ClusterError::replay_diverged(
                        position,
                        "a transaction in the epoch after the last close",
                    ));
                }
                let capacity = self.config.transactions_retained().get() as usize;
                if self.transactions.len() >= capacity {
                    return Err(ClusterError::full("multi-partition transactions", capacity));
                }
            }
        }
        self.hold(round.position(), record)
    }
}
