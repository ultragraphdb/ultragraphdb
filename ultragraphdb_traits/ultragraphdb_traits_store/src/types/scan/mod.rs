/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::direction::Direction;

/// What a [`crate::StoreSnapshot::scan`] reads: the keys of a prefix or of a range, from a seek point onwards, in a
/// direction, each with its newest version or with all its versions.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Scan {
    start: Vec<u8>,
    end: Option<Vec<u8>>,
    seek: Option<Vec<u8>>,
    direction: Direction,
    all_versions: bool,
}

impl Scan {
    /// The keys from `start`, inclusive, to `end`, exclusive, or to the last key when `end` is `None`.
    pub fn range(start: Vec<u8>, end: Option<Vec<u8>>) -> Self {
        Self {
            start,
            end,
            seek: None,
            direction: Direction::Forward,
            all_versions: false,
        }
    }

    /// The keys beginning with `prefix`; every key for an empty prefix.
    pub fn prefix(prefix: Vec<u8>) -> Self {
        let end = prefix_end(&prefix);
        Self::range(prefix, end)
    }

    /// The one key `key`.
    pub fn key(key: Vec<u8>) -> Self {
        let mut end = key.clone();
        end.push(0);
        Self::range(key, Some(end))
    }

    /// The scan starting at `key`: forward at the first key at or above it, in reverse at the last key at or below
    /// it, as Badger's `Iterator.Seek`.
    pub fn seek(mut self, key: Vec<u8>) -> Self {
        self.seek = Some(key);
        self
    }

    /// The scan in descending key order.
    pub fn reverse(mut self) -> Self {
        self.direction = Direction::Reverse;
        self
    }

    /// The scan reporting each of a key's versions, newest first, rather than its newest alone.
    pub fn all_versions(mut self) -> Self {
        self.all_versions = true;
        self
    }

    /// The first key of the range, inclusive.
    pub fn start(&self) -> &[u8] {
        &self.start
    }

    /// The end of the range, exclusive; `None` for no end.
    pub fn end(&self) -> Option<&[u8]> {
        self.end.as_deref()
    }

    pub fn seek_key(&self) -> Option<&[u8]> {
        self.seek.as_deref()
    }

    pub fn direction(&self) -> Direction {
        self.direction
    }

    pub fn is_all_versions(&self) -> bool {
        self.all_versions
    }

    /// Whether the scan reports `key`: inside the range and not before the seek point in the scan's direction.
    pub fn selects(&self, key: &[u8]) -> bool {
        let in_range =
            key >= self.start.as_slice() && self.end.as_deref().is_none_or(|end| key < end);
        let past_seek = self
            .seek
            .as_deref()
            .is_none_or(|seek| match self.direction {
                Direction::Forward => key >= seek,
                Direction::Reverse => key <= seek,
            });
        in_range && past_seek
    }
}

/// The first key above every key beginning with `prefix`: the prefix without its trailing `0xFF` bytes, its last
/// byte incremented; `None` when every byte is `0xFF`.
fn prefix_end(prefix: &[u8]) -> Option<Vec<u8>> {
    let last = prefix.iter().rposition(|&b| b != 0xFF)?;
    let mut end = prefix[..=last].to_vec();
    end[last] += 1;
    Some(end)
}
