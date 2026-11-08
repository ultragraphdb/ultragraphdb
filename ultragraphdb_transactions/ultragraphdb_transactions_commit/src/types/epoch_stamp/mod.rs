/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Epoch, Version};

/// A partition's commit versions (commit-rounds, "Commit versions are built from the epoch"): each admitted entry takes
/// the open epoch, exactly one above the latest close delivered to the partition, in its upper 42 bits and the next even
/// sequence within that epoch, from 0, in its lower 22. The stamp never advances its epoch on its own: once the open
/// epoch's sequences are spent, at 2^22 − 2, it is full and gives no version until the next close is delivered, so no
/// entry is ever stamped with an epoch whose close the partition holds. A close arrives only one above the last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EpochStamp {
    delivered: Epoch,
    next: Option<u32>,
    last: Option<Version>,
}

impl EpochStamp {
    /// The stamp of a partition whose latest delivered close is `delivered`, before any entry of its open epoch.
    #[must_use]
    pub fn new(delivered: Epoch) -> Self {
        Self {
            delivered,
            next: Some(0),
            last: None,
        }
    }

    /// The stamp of a partition whose latest delivered close is `delivered` and whose newest version stamped is `last`,
    /// as recovery finds them: the open epoch's next sequence follows `last` when `last` is of the open epoch.
    #[must_use]
    pub fn after(delivered: Epoch, last: Version) -> Self {
        Self {
            delivered,
            next: next_after(delivered, Some(last)),
            last: Some(last),
        }
    }

    /// Takes the close of `close`: `true` when it is the one above the latest delivered, which opens the epoch after it
    /// at sequence 0; `false`, changing nothing, for a close at or below the latest, delivered already.
    ///
    /// `None` for a close more than one above the latest: a gap, which the stamp refuses, changing nothing.
    pub fn deliver(&mut self, close: Epoch) -> Option<bool> {
        if close <= self.delivered {
            return Some(false);
        }
        let following = self.delivered.next().ok()?;
        if close != following {
            return None;
        }
        self.delivered = close;
        self.next = next_after(close, self.last);
        debug_assert!(self.next.is_none_or(|s| s.is_multiple_of(2)));
        Some(true)
    }

    /// The version of the next admitted entry, which becomes the newest stamped; `None` when the open epoch is full, or
    /// when no epoch follows the latest close.
    pub fn take(&mut self) -> Option<Version> {
        let version = self.peek()?;
        debug_assert!(
            self.last.is_none_or(|last| version > last),
            "versions strictly increase"
        );
        debug_assert!(version.is_commit(), "commit versions are even");
        self.next = version
            .sequence()
            .checked_add(2)
            .filter(|s| *s <= Version::COMMIT_SEQUENCE_MAX);
        self.last = Some(version);
        Some(version)
    }

    /// The version the next admitted entry would take, taking nothing; `None` as [`EpochStamp::take`] gives none.
    #[must_use]
    pub fn peek(&self) -> Option<Version> {
        Version::commit(self.open()?, self.next?).ok()
    }

    /// The latest close delivered.
    #[must_use]
    pub fn delivered(&self) -> Epoch {
        self.delivered
    }

    /// The open epoch, one above the latest close delivered; `None` past the last epoch.
    #[must_use]
    pub fn open(&self) -> Option<Epoch> {
        self.delivered.next().ok()
    }

    /// Whether the open epoch's sequences are spent, so no entry takes a version until the next close.
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.next.is_none()
    }

    /// The newest version stamped, `None` before the first.
    #[must_use]
    pub fn last(&self) -> Option<Version> {
        self.last
    }
}

/// The open epoch's next sequence after the close `delivered`, the newest version stamped being `last`: one after
/// `last` when `last` is of the open epoch, 0 when it is of an epoch before it or there is none, and none when it is of
/// an epoch after it, which no version may follow in an earlier epoch, or when no epoch follows the close.
fn next_after(delivered: Epoch, last: Option<Version>) -> Option<u32> {
    let open = delivered.next().ok()?;
    match last {
        Some(last) if last.epoch() == open => last
            .sequence()
            .checked_add(2)
            .filter(|s| *s <= Version::COMMIT_SEQUENCE_MAX),
        Some(last) if last.epoch() > open => None,
        Some(_) | None => Some(0),
    }
}
