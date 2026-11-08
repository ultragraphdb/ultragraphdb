/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod log_error_display;
mod log_error_error;

use std::io::ErrorKind;

/// Why an operation of the ordered log failed, grouped by what the caller does about it.
///
/// - **Retry or redirect:** [`NotPrimary`](Self::NotPrimary), [`Busy`](Self::Busy), [`Sealed`](Self::Sealed). The
///   round was not accepted; nothing was written.
/// - **Exhausted:** [`Exhausted`](Self::Exhausted). The log holds the last position a round can take; it accepts no
///   round ever again, and nothing was written.
/// - **Outcome unknown:** [`Truncated`](Self::Truncated). The round may or may not be durable; the client's session
///   resolves it.
/// - **Programming errors:** [`PositionMismatch`](Self::PositionMismatch), [`ParentMismatch`](Self::ParentMismatch),
///   [`TooLarge`](Self::TooLarge). The caller asked for something the log never accepts; nothing was written.
/// - **Fatal:** [`Corrupt`](Self::Corrupt), [`CorruptChunk`](Self::CorruptChunk),
///   [`SuperblockUnreadable`](Self::SuperblockUnreadable), [`Diverged`](Self::Diverged), [`Io`](Self::Io),
///   [`Poisoned`](Self::Poisoned). The node stops writing and does not start until the cause is repaired.
///
/// Positions are the rounds' gapless numbers, the first round at 1; a segment is named by its index, and an offset
/// is a byte offset within that segment's file. The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LogErrorEnum {
    /// This replica is not the primary, so it takes no appends; send them to the primary.
    NotPrimary,
    /// The log holds `segments` segments, its capacity: a checkpoint must trim it before another round fits.
    Busy {
        /// The segments the log holds, its capacity.
        segments: u64,
    },
    /// The log was sealed at `position`; a later round goes to the log that continues it.
    Sealed {
        /// The position the log was sealed at.
        position: u64,
    },
    /// The log holds a round at `position`, the last position a round can take (`u64::MAX`): no position follows it,
    /// so no round can be appended; nothing was written.
    Exhausted {
        /// The last position, `u64::MAX`.
        position: u64,
    },
    /// The round at `position` was appended and then replaced before it became durable; its outcome is resolved
    /// through the client's session.
    Truncated {
        /// The position of the replaced round.
        position: u64,
    },
    /// A position the operation does not take: an append's position is not `expected`, the one after the tail; a
    /// read starts outside the rounds the log holds, of which `expected` is the nearest bound; a trim reaches past
    /// `expected`, the committed position.
    PositionMismatch {
        /// The position the operation takes, or its nearest bound.
        expected: u64,
        /// The position asked for.
        found: u64,
    },
    /// The round at `position` names `found` as its parent, not `expected`, the checksum of the round before it.
    ParentMismatch {
        /// The round's position.
        position: u64,
        /// The checksum of the round before it.
        expected: u64,
        /// The parent the round names.
        found: u64,
    },
    /// A round of `size` bytes is larger than `limit`: the 1 MiB frame, or the segment it would have to fit in.
    TooLarge {
        /// The round's size in bytes.
        size: usize,
        /// The largest size that fits.
        limit: usize,
    },
    /// The round expected at `position`, at byte `offset` of segment `segment`, is damaged before its body: its
    /// header, its place in the chain, or its chunk table. A complete round follows it, so it is not a torn tail.
    Corrupt {
        /// The round's position.
        position: u64,
        /// The segment holding the round.
        segment: u64,
        /// The round's byte offset within the segment.
        offset: u64,
    },
    /// Chunk `chunk`, body bytes `start` to `end`, of the round at `position`, at byte `offset` of segment
    /// `segment`, does not match its leaf, and a complete round follows it. The transactions whose records overlap
    /// the range are the round body's to name.
    CorruptChunk {
        /// The round's position.
        position: u64,
        /// The segment holding the round.
        segment: u64,
        /// The round's byte offset within the segment.
        offset: u64,
        /// The chunk's index in the round's chunk table.
        chunk: usize,
        /// The first body byte of the chunk.
        start: usize,
        /// The body byte after the chunk's last.
        end: usize,
    },
    /// Superblock copy `copy`, `'a'` or `'b'`, holds bytes that do not verify. Its sequence is lost, so it may have
    /// been the newer copy: the log neither starts nor writes, and it is never read as a fresh log.
    SuperblockUnreadable {
        /// The copy, `'a'` or `'b'`.
        copy: char,
    },
    /// The log does not match what it must: `what` (the cluster, the checkpoint's checksum, the first round after
    /// the checkpoint, the segments the superblock counts) is `found`, not `expected`.
    Diverged {
        /// What the log does not match.
        what: &'static str,
        /// The value it must hold.
        expected: u64,
        /// The value it holds.
        found: u64,
    },
    /// The storage failed `operation` with an error of `kind` saying `message`. A failed write or sync poisons the
    /// log.
    Io {
        /// The storage operation that failed.
        operation: &'static str,
        /// The kind of the I/O error.
        kind: ErrorKind,
        /// The I/O error's text.
        message: String,
    },
    /// A write or sync failed earlier; the log refuses every operation until it is opened again.
    Poisoned,
}

/// Why an operation of the ordered log failed; see [`LogErrorEnum`] for what each variant asks of the caller.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LogError(pub LogErrorEnum);

impl LogError {
    /// This replica is not the primary.
    #[must_use]
    pub fn not_primary() -> Self {
        Self(LogErrorEnum::NotPrimary)
    }

    /// The log holds `segments` segments, its capacity.
    #[must_use]
    pub fn busy(segments: u64) -> Self {
        Self(LogErrorEnum::Busy { segments })
    }

    /// The log was sealed at `position`.
    #[must_use]
    pub fn sealed(position: u64) -> Self {
        Self(LogErrorEnum::Sealed { position })
    }

    /// The log holds a round at `position`, the last position a round can take.
    #[must_use]
    pub fn exhausted(position: u64) -> Self {
        Self(LogErrorEnum::Exhausted { position })
    }

    /// The round at `position` was replaced before it became durable.
    #[must_use]
    pub fn truncated(position: u64) -> Self {
        Self(LogErrorEnum::Truncated { position })
    }

    /// A position the operation does not take: `found`, where `expected` is the one it takes or its nearest bound.
    #[must_use]
    pub fn position_mismatch(expected: u64, found: u64) -> Self {
        Self(LogErrorEnum::PositionMismatch { expected, found })
    }

    /// The round at `position` names `found` as its parent, not `expected`.
    #[must_use]
    pub fn parent_mismatch(position: u64, expected: u64, found: u64) -> Self {
        Self(LogErrorEnum::ParentMismatch {
            position,
            expected,
            found,
        })
    }

    /// A round of `size` bytes is larger than `limit`.
    #[must_use]
    pub fn too_large(size: usize, limit: usize) -> Self {
        Self(LogErrorEnum::TooLarge { size, limit })
    }

    /// The round at `position`, at byte `offset` of segment `segment`, is damaged before its body.
    #[must_use]
    pub fn corrupt(position: u64, segment: u64, offset: u64) -> Self {
        Self(LogErrorEnum::Corrupt {
            position,
            segment,
            offset,
        })
    }

    /// Chunk `chunk`, body bytes `start` to `end`, of the round at `position` does not match its leaf.
    #[must_use]
    pub fn corrupt_chunk(
        position: u64,
        segment: u64,
        offset: u64,
        chunk: usize,
        start: usize,
        end: usize,
    ) -> Self {
        Self(LogErrorEnum::CorruptChunk {
            position,
            segment,
            offset,
            chunk,
            start,
            end,
        })
    }

    /// Superblock copy `copy` holds bytes that do not verify.
    #[must_use]
    pub fn superblock_unreadable(copy: char) -> Self {
        Self(LogErrorEnum::SuperblockUnreadable { copy })
    }

    /// The log's `what` is `found`, not `expected`.
    #[must_use]
    pub fn diverged(what: &'static str, expected: u64, found: u64) -> Self {
        Self(LogErrorEnum::Diverged {
            what,
            expected,
            found,
        })
    }

    /// The storage failed `operation` with an error of `kind` saying `message`.
    pub fn io(operation: &'static str, kind: ErrorKind, message: impl Into<String>) -> Self {
        Self(LogErrorEnum::Io {
            operation,
            kind,
            message: message.into(),
        })
    }

    /// A write or sync failed earlier; the log refuses every operation until reopened.
    #[must_use]
    pub fn poisoned() -> Self {
        Self(LogErrorEnum::Poisoned)
    }
}
