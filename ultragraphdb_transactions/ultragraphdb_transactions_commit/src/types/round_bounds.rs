/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::MAX_FRAME_SIZE;

/// The bounds a round closes within: its frame's size in bytes, header and chunk table included, and its number of
/// transactions. Their defaults follow the log's power-loss-protection flag and are the write-ahead log's
/// `WalConfig::round_bytes` and `round_transactions`, which the node passes here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RoundBounds {
    bytes: u32,
    transactions: u32,
}

impl RoundBounds {
    /// At most `bytes` of frame, taken as the maximum frame when larger, and at most `transactions`, taken as 1
    /// when 0, so a round always closes.
    pub fn new(bytes: u32, transactions: u32) -> Self {
        Self {
            bytes: bytes.min(MAX_FRAME_SIZE as u32),
            transactions: transactions.max(1),
        }
    }

    pub fn bytes(&self) -> u32 {
        self.bytes
    }

    pub fn transactions(&self) -> u32 {
        self.transactions
    }
}
