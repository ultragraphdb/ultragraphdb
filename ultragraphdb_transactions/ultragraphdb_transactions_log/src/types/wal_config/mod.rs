/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU32;

use crate::utils::block::BLOCK;

/// 16,384 blocks, 64 MiB.
const SEGMENT_BLOCKS: NonZeroU32 = NonZeroU32::new(16_384).expect("16,384 is not zero");

/// 64 segments, 4 GiB of rounds at most.
const CAPACITY: NonZeroU32 = NonZeroU32::new(64).expect("64 is not zero");

/// The write-ahead log's configuration: the cluster its rounds belong to, its segments, and the knobs a sync's cost
/// decides.
///
/// **The power-loss-protection flag** states whether the log's drive has power-loss protection, `false` unless a
/// deployment says otherwise. It sets the defaults of the round bounds and of the checkpoint interval and nothing
/// else: every round is synced before it is acknowledged whatever it says, so a flag set wrongly costs throughput or
/// latency, never an acknowledged round.
///
/// | Knob | Without protection | With protection |
/// |---|---|---|
/// | Round bytes | 1,048,576 (the frame) | 262,144 |
/// | Round transactions | 1,024 | 256 |
/// | Checkpoint interval | 268,435,456 bytes of rounds | 67,108,864 |
///
/// A sync costs milliseconds on a drive without protection and microseconds on one with it, so without it rounds
/// grow to carry more transactions per sync, and checkpoints, each a store sync, are spaced further apart. The values
/// are provisional until task 12.4 measures both kinds of drive. A segment is 16,384 blocks of 4,096 bytes (64 MiB)
/// and the log holds at most 64 segments, whatever the flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WalConfig {
    cluster: u64,
    power_loss_protection: bool,
    segment_blocks: NonZeroU32,
    capacity: NonZeroU32,
    round_bytes: u32,
    round_transactions: u32,
    checkpoint_bytes: u64,
}

impl WalConfig {
    /// The defaults for `cluster` on a drive with or without power-loss protection.
    pub fn new(cluster: u64, power_loss_protection: bool) -> Self {
        // Provisional until task 12.4 measures both kinds of drive; see the table above.
        let (round_bytes, round_transactions, checkpoint_bytes) = if power_loss_protection {
            (262_144, 256, 67_108_864)
        } else {
            (1_048_576, 1_024, 268_435_456)
        };
        Self {
            cluster,
            power_loss_protection,
            segment_blocks: SEGMENT_BLOCKS,
            capacity: CAPACITY,
            round_bytes,
            round_transactions,
            checkpoint_bytes,
        }
    }

    /// Segments of `blocks` blocks of 4,096 bytes; a round fits in one segment or is refused.
    pub fn with_segment_blocks(self, blocks: NonZeroU32) -> Self {
        Self {
            segment_blocks: blocks,
            ..self
        }
    }

    /// At most `segments` segments kept; an append past them waits for a checkpoint.
    pub fn with_capacity(self, segments: NonZeroU32) -> Self {
        Self {
            capacity: segments,
            ..self
        }
    }

    /// Rounds of at most `bytes` bytes of write-set records, for the commit step; the 1 MiB frame bounds every round
    /// whatever this says.
    pub fn with_round_bytes(self, bytes: u32) -> Self {
        Self {
            round_bytes: bytes,
            ..self
        }
    }

    /// Rounds of at most `transactions` transactions, for the commit step.
    pub fn with_round_transactions(self, transactions: u32) -> Self {
        Self {
            round_transactions: transactions,
            ..self
        }
    }

    /// A checkpoint after every `bytes` bytes of rounds, for the commit step.
    pub fn with_checkpoint_bytes(self, bytes: u64) -> Self {
        Self {
            checkpoint_bytes: bytes,
            ..self
        }
    }

    pub fn cluster(&self) -> u64 {
        self.cluster
    }

    pub fn power_loss_protection(&self) -> bool {
        self.power_loss_protection
    }

    pub fn segment_blocks(&self) -> NonZeroU32 {
        self.segment_blocks
    }

    /// A segment's length in bytes.
    pub fn segment_size(&self) -> u64 {
        u64::from(self.segment_blocks.get()) * BLOCK as u64
    }

    pub fn capacity(&self) -> NonZeroU32 {
        self.capacity
    }

    pub fn round_bytes(&self) -> u32 {
        self.round_bytes
    }

    pub fn round_transactions(&self) -> u32 {
        self.round_transactions
    }

    pub fn checkpoint_bytes(&self) -> u64 {
        self.checkpoint_bytes
    }
}
