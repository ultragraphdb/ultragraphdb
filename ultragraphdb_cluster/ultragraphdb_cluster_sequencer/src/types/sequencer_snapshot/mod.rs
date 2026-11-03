/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod sequencer_snapshot_codec;

use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_traits_log::Position;

use crate::types::close::Close;
use crate::types::sequenced_transaction::SequencedTransaction;

/// The sequencer's durable state as of a log position, from which recovery replays only the rounds after it: the
/// closes it retains, in epoch order and consecutive, and the multi-partition transactions it holds, each at the log
/// position it was appended at. The host stores its encoding ([`SequencerSnapshot::encode`]) durably before it trims
/// the log through its position ([`Sequencer::trim`](crate::Sequencer::trim)).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SequencerSnapshot {
    position: Position,
    closes: Vec<Close>,
    transactions: Vec<(Position, SequencedTransaction)>,
}

/// Which part of a snapshot a value out of order lies in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Part {
    /// The closes.
    Closes,
    /// The transactions.
    Transactions,
}

impl SequencerSnapshot {
    /// The snapshot at `position` holding `closes` and `transactions`.
    ///
    /// # Errors
    ///
    /// [`ClusterError::malformed`] naming the first value out of order and its index among the closes or the
    /// transactions: a close not one after the one before it or recorded before it, a transaction not after the one
    /// before it (the first after position 0), or past `position`.
    pub fn new(
        position: Position,
        closes: Vec<Close>,
        transactions: Vec<(Position, SequencedTransaction)>,
    ) -> Result<Self, ClusterError> {
        if let Some((_, index, what)) = first_disorder(position, &closes, &transactions) {
            return Err(ClusterError::malformed(what, index));
        }
        Ok(Self {
            position,
            closes,
            transactions,
        })
    }

    /// The log position the snapshot was taken at.
    #[must_use]
    pub fn position(&self) -> Position {
        self.position
    }

    /// The closes retained, in epoch order.
    #[must_use]
    pub fn closes(&self) -> &[Close] {
        &self.closes
    }

    /// The multi-partition transactions held, each at its log position, in log order.
    #[must_use]
    pub fn transactions(&self) -> &[(Position, SequencedTransaction)] {
        &self.transactions
    }

    /// The snapshot of a sequencer's held records, which are in order by the sequencer's own checks.
    pub(crate) fn from_held(
        position: Position,
        closes: Vec<Close>,
        transactions: Vec<(Position, SequencedTransaction)>,
    ) -> Self {
        debug_assert!(first_disorder(position, &closes, &transactions).is_none());
        Self {
            position,
            closes,
            transactions,
        }
    }

    /// The parts, the snapshot consumed.
    pub(crate) fn into_parts(
        self,
    ) -> (Position, Vec<Close>, Vec<(Position, SequencedTransaction)>) {
        (self.position, self.closes, self.transactions)
    }
}

/// The first value out of order in a snapshot of these parts: its part, its index there and the rule it breaks.
pub(crate) fn first_disorder(
    position: Position,
    closes: &[Close],
    transactions: &[(Position, SequencedTransaction)],
) -> Option<(Part, usize, &'static str)> {
    for (index, pair) in closes.windows(2).enumerate() {
        if let [before, after] = pair {
            if after.epoch().get() != before.epoch().get() + 1 {
                return Some((
                    Part::Closes,
                    index + 1,
                    "a close one after the one before it",
                ));
            }
            if after.time_us() < before.time_us() {
                return Some((
                    Part::Closes,
                    index + 1,
                    "a close time at or above the one before it",
                ));
            }
        }
    }
    let mut before = Position::ZERO;
    for (index, (at, _)) in transactions.iter().enumerate() {
        if *at <= before {
            return Some((
                Part::Transactions,
                index,
                "a transaction after the one before it",
            ));
        }
        if *at > position {
            return Some((
                Part::Transactions,
                index,
                "a transaction at or before the snapshot's position",
            ));
        }
        before = *at;
    }
    None
}
