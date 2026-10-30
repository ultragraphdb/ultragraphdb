/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::MAX_FRAME_SIZE;

use crate::types::commit_step::frame_len;

/// The bounds a round closes within: its frame's size in bytes, header and chunk table included, and its number of
/// transactions. Their defaults follow the log's power-loss-protection flag and are the write-ahead log's
/// `WalConfig::round_bytes` and `round_transactions`, which the node passes here.
///
/// The largest write-set record a round holds on its own, [`RoundBounds::max_write_set`], is what the node announces as
/// the Welcome's `max_write_set` and bounds its transactions' records by: a record the commit step accepts always fits a
/// round, 1,046,224 bytes of a round of 1,048,576 and 261,328 of one of 262,144.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RoundBounds {
    bytes: u32,
    transactions: u32,
    max_write_set: u32,
}

impl RoundBounds {
    /// At most `bytes` of frame, taken as the maximum frame when larger, and at most `transactions`, taken as 1
    /// when 0, so a round always closes.
    pub fn new(bytes: u32, transactions: u32) -> Self {
        let bytes = bytes.min(MAX_FRAME_SIZE as u32);
        Self {
            bytes,
            transactions: transactions.max(1),
            // At most `bytes`, far below u32.
            max_write_set: largest_record(bytes as usize) as u32,
        }
    }

    pub fn bytes(&self) -> u32 {
        self.bytes
    }

    pub fn transactions(&self) -> u32 {
        self.transactions
    }

    /// The largest write-set record a round of these bounds holds on its own, its framing counted: what the node
    /// announces as the Welcome's `max_write_set` and bounds its transactions' records by. 0 when not even an empty
    /// record's round fits.
    pub fn max_write_set(&self) -> usize {
        self.max_write_set as usize
    }
}

/// The largest record a round of it alone holds within `bytes` of frame, 0 when none does: the frame grows with the
/// record, so a binary search over lengths finds it.
fn largest_record(bytes: usize) -> usize {
    let fits = |len: usize| frame_len(1, 1, len) <= bytes;
    if !fits(0) {
        return 0;
    }
    let (mut low, mut high) = (0, bytes);
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        if fits(mid) {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    low
}
