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
    Busy { segments: u64 },
    /// The log was sealed at `position`; a later round goes to the log that continues it.
    Sealed { position: u64 },
    /// The round at `position` was appended and then replaced before it became durable; its outcome is resolved
    /// through the client's session.
    Truncated { position: u64 },
    /// A position the operation does not take: an append's position is not `expected`, the one after the tail; a
    /// read starts outside the rounds the log holds, of which `expected` is the nearest bound; a trim reaches past
    /// `expected`, the committed position.
    PositionMismatch { expected: u64, found: u64 },
    /// The round at `position` names `found` as its parent, not `expected`, the checksum of the round before it.
    ParentMismatch {
        position: u64,
        expected: u64,
        found: u64,
    },
    /// A round of `size` bytes is larger than `limit`: the 1 MiB frame, or the segment it would have to fit in.
    TooLarge { size: usize, limit: usize },
    /// The round expected at `position`, at byte `offset` of segment `segment`, is damaged before its body: its
    /// header, its place in the chain, or its chunk table. A complete round follows it, so it is not a torn tail.
    Corrupt {
        position: u64,
        segment: u64,
        offset: u64,
    },
    /// Chunk `chunk`, body bytes `start` to `end`, of the round at `position`, at byte `offset` of segment
    /// `segment`, does not match its leaf, and a complete round follows it. The transactions whose records overlap
    /// the range are the round body's to name.
    CorruptChunk {
        position: u64,
        segment: u64,
        offset: u64,
        chunk: usize,
        start: usize,
        end: usize,
    },
    /// Superblock copy `copy`, `'a'` or `'b'`, holds bytes that do not verify. Its sequence is lost, so it may have
    /// been the newer copy: the log neither starts nor writes, and it is never read as a fresh log.
    SuperblockUnreadable { copy: char },
    /// The log does not match what it must: `what` (the cluster, the checkpoint's checksum, the first round after
    /// the checkpoint, the segments the superblock counts) is `found`, not `expected`.
    Diverged {
        what: &'static str,
        expected: u64,
        found: u64,
    },
    /// The storage failed `operation` with an error of `kind` saying `message`. A failed write or sync poisons the
    /// log.
    Io {
        operation: &'static str,
        kind: ErrorKind,
        message: String,
    },
    /// A write or sync failed earlier; the log refuses every operation until it is opened again.
    Poisoned,
}

/// Why an operation of the ordered log failed; see [`LogErrorEnum`] for what each variant asks of the caller.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LogError(pub LogErrorEnum);

impl LogError {
    #[allow(non_snake_case)]
    pub fn NotPrimary() -> Self {
        Self(LogErrorEnum::NotPrimary)
    }

    #[allow(non_snake_case)]
    pub fn Busy(segments: u64) -> Self {
        Self(LogErrorEnum::Busy { segments })
    }

    #[allow(non_snake_case)]
    pub fn Sealed(position: u64) -> Self {
        Self(LogErrorEnum::Sealed { position })
    }

    #[allow(non_snake_case)]
    pub fn Truncated(position: u64) -> Self {
        Self(LogErrorEnum::Truncated { position })
    }

    #[allow(non_snake_case)]
    pub fn PositionMismatch(expected: u64, found: u64) -> Self {
        Self(LogErrorEnum::PositionMismatch { expected, found })
    }

    #[allow(non_snake_case)]
    pub fn ParentMismatch(position: u64, expected: u64, found: u64) -> Self {
        Self(LogErrorEnum::ParentMismatch {
            position,
            expected,
            found,
        })
    }

    #[allow(non_snake_case)]
    pub fn TooLarge(size: usize, limit: usize) -> Self {
        Self(LogErrorEnum::TooLarge { size, limit })
    }

    #[allow(non_snake_case)]
    pub fn Corrupt(position: u64, segment: u64, offset: u64) -> Self {
        Self(LogErrorEnum::Corrupt {
            position,
            segment,
            offset,
        })
    }

    #[allow(non_snake_case)]
    pub fn CorruptChunk(
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

    #[allow(non_snake_case)]
    pub fn SuperblockUnreadable(copy: char) -> Self {
        Self(LogErrorEnum::SuperblockUnreadable { copy })
    }

    #[allow(non_snake_case)]
    pub fn Diverged(what: &'static str, expected: u64, found: u64) -> Self {
        Self(LogErrorEnum::Diverged {
            what,
            expected,
            found,
        })
    }

    #[allow(non_snake_case)]
    pub fn Io(operation: &'static str, kind: ErrorKind, message: impl Into<String>) -> Self {
        Self(LogErrorEnum::Io {
            operation,
            kind,
            message: message.into(),
        })
    }

    #[allow(non_snake_case)]
    pub fn Poisoned() -> Self {
        Self(LogErrorEnum::Poisoned)
    }
}
