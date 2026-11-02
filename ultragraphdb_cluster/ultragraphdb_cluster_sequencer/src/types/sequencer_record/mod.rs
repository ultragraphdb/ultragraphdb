/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod sequencer_record_codec;

use ultragraphdb_core_ids::Epoch;

use crate::types::close::Close;
use crate::types::sequenced_transaction::SequencedTransaction;

/// One record of the sequencer's log, the body of one round: a close or a multi-partition transaction. Its encoding
/// is [`SequencerRecord::encode`]'s.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SequencerRecord {
    /// The close of an epoch.
    Close(Close),
    /// A multi-partition transaction placed in the open epoch.
    Transaction(SequencedTransaction),
}

impl SequencerRecord {
    /// The epoch the record closes or places its transaction in.
    #[must_use]
    pub fn epoch(&self) -> Epoch {
        match self {
            Self::Close(close) => close.epoch(),
            Self::Transaction(transaction) => transaction.epoch(),
        }
    }
}
