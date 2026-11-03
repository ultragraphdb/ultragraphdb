/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

use ultragraphdb_core_ids::Epoch;
use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_traits_log::{OrderedLog, Position, Round};

use crate::types::clock_reading::ClockReading;
use crate::types::sequencer::{Sequencer, epoch_of};
use crate::types::sequencer_config::SequencerConfig;
use crate::types::sequencer_record::SequencerRecord;
use crate::types::sequencer_snapshot::SequencerSnapshot;

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
    /// [`ClusterError::snapshot_behind`] when the log is trimmed, its checkpoint past position 0, which only
    /// [`recover_from`](Self::recover_from) with a snapshot recovers; [`ClusterError::log`] when the log cannot be
    /// read; [`ClusterError::malformed`] when a round's body is no record; [`ClusterError::replay_diverged`] when a
    /// round contradicts the ones before it (a close not one after the
    /// last, a close time below the last, a transaction not in the epoch after the last close, a round's version not
    /// its record's epoch); [`ClusterError::full`] when the log holds more transactions than the settings retain.
    pub async fn recover(
        log: L,
        config: SequencerConfig,
        now: ClockReading,
    ) -> Result<Self, ClusterError> {
        Self::recover_from(log, config, now, None).await
    }

    /// The sequencer over `log`, its state rebuilt from `snapshot`, the last the host stored, by replaying every
    /// durable round after the snapshot's position in order; with no snapshot, every durable round. The next close is
    /// due `epoch_ms_max` after `now`.
    ///
    /// # Errors
    ///
    /// [`ClusterError::snapshot_behind`] when the snapshot, or position 0 without one, lies below the log's checkpoint;
    /// [`ClusterError::replay_diverged`] when the snapshot lies past the log's committed position; the refusals of
    /// [`recover`](Self::recover) for the rounds after it.
    pub async fn recover_from(
        log: L,
        config: SequencerConfig,
        now: ClockReading,
        snapshot: Option<SequencerSnapshot>,
    ) -> Result<Self, ClusterError> {
        let (base, closes, transactions) = snapshot.map_or_else(
            || (Position::ZERO, Vec::new(), Vec::new()),
            SequencerSnapshot::into_parts,
        );
        let checkpoint = log.checkpoint();
        if base < checkpoint {
            return Err(ClusterError::snapshot_behind(base.get(), checkpoint.get()));
        }
        if base > log.committed() {
            return Err(ClusterError::replay_diverged(
                base.get(),
                "a snapshot at or below the log's committed position",
            ));
        }
        let mut sequencer = Self {
            log,
            config,
            closes: closes.into(),
            transactions: transactions.into(),
            durable: base,
            closed: Epoch::ZERO,
            last_close_us: now.monotonic_us(),
            close_requested: false,
            subscribers: BTreeMap::new(),
            buffer: Vec::new(),
        };
        sequencer.replay().await?;
        Ok(sequencer)
    }

    /// Replays every durable round after the durable position, the snapshot's, in order, then counts every record held
    /// as durable.
    async fn replay(&mut self) -> Result<(), ClusterError> {
        let committed = self.log.committed();
        let mut next = Position::new(self.durable.get() + 1);
        // Each read returns at least one round while any is left, so the rounds bound the reads.
        for _ in 0..committed.get().saturating_sub(self.durable.get()) {
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
