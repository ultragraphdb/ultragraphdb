/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU32;

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::command::Command;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::le::bytes_at;

/// Where the command header's unused bytes start, after `derivation`.
const UNUSED_FROM: usize = 40;

/// The command header of a Round frame, the log record of one partition's round (`PROTOCOL.md` §12.1): what makes a
/// round self-identifying, so a scan of the log stops at the first block that is not the next round of this log, and
/// the derivation that turns its quads into keys.
///
/// | Offset | Field | Type |
/// |---|---|---|
/// | 0 | `position` | u64, the round's place in the log, gapless from 1 |
/// | 8 | `parent` | u64, the previous round's frame `checksum` |
/// | 16 | `incarnation` | u64, the log's incarnation, raised each time a log is created afresh |
/// | 24 | `first_version` | u64, the commit version of the round's first committed entry |
/// | 32 | `transactions` | u32, the decision count, one per entry |
/// | 36 | `derivation` | u32, the version of the derivation that turns the round's quads into keys, at least 1 |
/// | 40 | unused | 88 bytes, zero |
///
/// Offsets are within the command header, the frame's bytes 128 to 256; every field is little-endian. The cluster
/// is the frame header's own field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RoundHeader {
    position: u64,
    parent: u64,
    incarnation: u64,
    first_version: u64,
    transactions: u32,
    derivation: NonZeroU32,
}

impl RoundHeader {
    /// The header of the round at `position` after the round of checksum `parent`, of the log's `incarnation`, its
    /// first committed entry at `first_version`, holding `transactions` decisions, its quads turned into keys by
    /// `derivation`.
    pub fn new(
        position: u64,
        parent: u64,
        incarnation: u64,
        first_version: u64,
        transactions: u32,
        derivation: NonZeroU32,
    ) -> Self {
        Self {
            position,
            parent,
            incarnation,
            first_version,
            transactions,
            derivation,
        }
    }

    /// The fields of `command_header`, checked in order.
    ///
    /// # Errors
    ///
    /// [`ProtocolError::reserved_not_zero`] at the frame offset of the first nonzero byte of 40 to 127; then
    /// [`ProtocolError::field_value`] `"derivation"` for a derivation of 0.
    pub fn decode(command_header: &[u8; COMMAND_HEADER_SIZE]) -> Result<Self, ProtocolError> {
        if let Some(at) = command_header
            .iter()
            .skip(UNUSED_FROM)
            .position(|byte| *byte != 0)
        {
            return Err(ProtocolError::reserved_not_zero(
                COMMAND_HEADER_SIZE + UNUSED_FROM + at,
            ));
        }
        let u64_at = |at: usize| bytes_at(command_header, at).map_or(0, u64::from_le_bytes);
        let u32_at = |at: usize| bytes_at(command_header, at).map_or(0, u32::from_le_bytes);
        let derivation = u32_at(36);
        let derivation = NonZeroU32::new(derivation).ok_or_else(|| {
            ProtocolError::field_value(Command::Round.number(), "derivation", derivation.into())
        })?;
        Ok(Self::new(
            u64_at(0),
            u64_at(8),
            u64_at(16),
            u64_at(24),
            u32_at(32),
            derivation,
        ))
    }

    /// The command header's 128 bytes, the unused ones zero.
    pub fn encode(&self) -> [u8; COMMAND_HEADER_SIZE] {
        let fields = self
            .position
            .to_le_bytes()
            .into_iter()
            .chain(self.parent.to_le_bytes())
            .chain(self.incarnation.to_le_bytes())
            .chain(self.first_version.to_le_bytes())
            .chain(self.transactions.to_le_bytes())
            .chain(self.derivation.get().to_le_bytes());
        let mut header = [0u8; COMMAND_HEADER_SIZE];
        header
            .iter_mut()
            .zip(fields)
            .for_each(|(out, byte)| *out = byte);
        header
    }

    /// The round's place in the log, gapless from 1.
    pub fn position(&self) -> u64 {
        self.position
    }

    /// The previous round's frame checksum.
    pub fn parent(&self) -> u64 {
        self.parent
    }

    /// The log's incarnation, raised each time a log is created afresh.
    pub fn incarnation(&self) -> u64 {
        self.incarnation
    }

    /// The commit version of the round's first committed entry.
    pub fn first_version(&self) -> u64 {
        self.first_version
    }

    /// The round's decision count, one per entry.
    pub fn transactions(&self) -> u32 {
        self.transactions
    }

    /// The version of the derivation that turns the round's quads into keys, at least 1.
    #[must_use]
    pub fn derivation(&self) -> NonZeroU32 {
        self.derivation
    }
}
