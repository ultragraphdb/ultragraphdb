/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod sequencer_as_of;
mod sequencer_close;
mod sequencer_deliver;
mod sequencer_recover;
mod sequencer_submit;

use std::collections::{BTreeMap, VecDeque};
use std::num::NonZeroU32;

use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_error_invariant::invariant;
use ultragraphdb_error_log::LogError;
use ultragraphdb_traits_log::{OrderedLog, Position, RoundRef};

use crate::types::close::Close;
use crate::types::sequenced_transaction::SequencedTransaction;
use crate::types::sequencer_config::SequencerConfig;
use crate::types::sequencer_record::SequencerRecord;

/// The derivation version the sequencer's rounds name: they hold no quads, so the first.
const DERIVATION: NonZeroU32 = NonZeroU32::MIN;

/// The sequencer: a state machine over the ordered log `L` that closes epochs and orders multi-partition
/// transactions, with one replica in this stage behind the interface stage 8 replicates.
///
/// Its state is a function of its log. Every close ([`tick`](Self::tick)) and every multi-partition transaction
/// ([`submit`](Self::submit)) is appended as a [`SequencerRecord`](crate::SequencerRecord), one per round, and counts
/// only once [`sync`](Self::sync) has made it durable: a close is delivered to subscribers, mapped from a time and
/// counted as the last closed epoch only then, and a transaction is read back only then. [`recover`](Self::recover)
/// rebuilds the state by replaying the log. The methods that decide are synchronous and do no I/O; `recover` and
/// `sync` wait on the log. Time is the caller's ([`ClockReading`](crate::ClockReading)): the sequencer next needs it at
/// [`deadline`](Self::deadline). Control plane: a record is encoded into one reused buffer, and the closes and
/// transactions held are bounded by the settings.
pub struct Sequencer<L: OrderedLog> {
    log: L,
    config: SequencerConfig,
    /// The last closes appended, in epoch order, consecutive; the newest is the last close appended, and every close
    /// held is durable once a sync has succeeded after it.
    closes: VecDeque<Close>,
    /// The multi-partition transactions appended, in log order, each at its log position.
    transactions: VecDeque<(Position, SequencedTransaction)>,
    /// The last durable position, as the latest sync reported it.
    durable: Position,
    /// The last epoch whose close is durable, 0 before the first.
    closed: Epoch,
    /// The monotonic time of the last close appended, or of the start before the first.
    last_close_us: u64,
    /// Whether a close of the open epoch has been asked for.
    close_requested: bool,
    /// Each subscribed partition with the last epoch delivered to it.
    subscribers: BTreeMap<PartitionId, Epoch>,
    /// The buffer every record is encoded into, reused.
    buffer: Vec<u8>,
}

/// The epoch `epoch`, a number of at most 42 bits by the caller's construction.
fn epoch_of(epoch: u64) -> Epoch {
    debug_assert!(epoch <= Epoch::MAX.get(), "an epoch number holds 42 bits");
    Epoch::new(epoch).unwrap_or(Epoch::MAX)
}

impl<L: OrderedLog> Sequencer<L> {
    /// The settings the sequencer runs with.
    #[must_use]
    pub fn config(&self) -> &SequencerConfig {
        &self.config
    }

    /// The last epoch whose close is durable, epoch 0 before the first.
    #[must_use]
    pub fn last_closed(&self) -> Epoch {
        self.closed
    }

    /// The monotonic time in microseconds at which the next close is due: `epoch_ms_max` after the last close, or
    /// `epoch_ms_min` after it once a close of the open epoch has been asked for.
    #[must_use]
    pub fn deadline(&self) -> u64 {
        let wait_ms = if self.close_requested {
            self.config.epoch_ms_min()
        } else {
            self.config.epoch_ms_max()
        };
        // At most 10,000 ms, so the product cannot overflow; the sum saturates at the end of time.
        self.last_close_us.saturating_add(wait_ms * 1_000)
    }

    /// Makes every appended record durable and returns the durable position: closes up to it are delivered, and
    /// transactions up to it acknowledged.
    ///
    /// # Errors
    ///
    /// [`ClusterError::log`] carrying the log's refusal; a failed sync poisons the log, so every later append fails.
    pub async fn sync(&mut self) -> Result<Position, ClusterError> {
        let durable = self.log.sync().await?;
        self.durable = durable;
        if let Some(last) = self.closes.back() {
            self.closed = last.epoch();
        }
        self.trim_closes();
        Ok(durable)
    }

    /// The epoch of the last close appended, 0 before the first.
    fn last_appended(&self) -> u64 {
        self.closes.back().map_or(0, |close| close.epoch().get())
    }

    /// The recorded time of the last close appended, 0 before the first.
    fn last_time_us(&self) -> u64 {
        self.closes.back().map_or(0, Close::time_us)
    }

    /// Drops the oldest closes past the number retained; called once every close held is durable.
    fn trim_closes(&mut self) {
        let retained = self.config.closes_retained().get() as usize;
        while self.closes.len() > retained {
            let _dropped: Option<Close> = self.closes.pop_front();
        }
    }

    /// Appends `record` as the round after the log's tail, its version the record's epoch, and holds it.
    fn append(&mut self, record: SequencerRecord) -> Result<Position, ClusterError> {
        self.buffer.clear();
        record.encode(&mut self.buffer);
        let (tail, parent) = self.log.tail();
        let position = tail.next().ok_or_else(|| LogError::exhausted(tail.get()))?;
        let transactions = match &record {
            SequencerRecord::Close(_) => 0,
            SequencerRecord::Transaction(_) => 1,
        };
        let round = RoundRef::new(
            position,
            parent,
            record.epoch().get(),
            transactions,
            DERIVATION,
            &self.buffer,
        );
        let appended = self.log.append(round)?;
        invariant(
            appended.position() == position,
            "a round is appended after the tail",
        )?;
        self.hold(position, record)?;
        Ok(position)
    }

    /// Holds `record`, appended at `position`: a close one after the last held, at or after its time, or a
    /// transaction in the epoch after the last close appended.
    fn hold(&mut self, position: Position, record: SequencerRecord) -> Result<(), ClusterError> {
        let last = self.last_appended();
        match record {
            SequencerRecord::Close(close) => {
                let follows = self
                    .closes
                    .back()
                    .is_none_or(|_| close.epoch().get() == last + 1);
                invariant(follows, "a close follows the last close by one epoch")?;
                invariant(
                    close.time_us() >= self.last_time_us(),
                    "close times never fall",
                )?;
                self.closes.push_back(close);
            }
            SequencerRecord::Transaction(transaction) => {
                let placed = transaction.epoch().get() == last + 1;
                invariant(
                    placed,
                    "a transaction lies in the epoch after the last close",
                )?;
                self.transactions.push_back((position, transaction));
            }
        }
        Ok(())
    }
}
