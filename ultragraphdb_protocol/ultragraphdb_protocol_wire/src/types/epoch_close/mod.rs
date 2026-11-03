/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod epoch_close_body_builder;

use crate::types::catalog_announcement::CatalogAnnouncement;

/// One of the sequencer's closes (`PROTOCOL.md` §13.4), on a Subscribe's stream: the epoch, the close's time, the
/// catalog versions it assigned, the placement version it assigned (0 for none) and the sessions evicted since the
/// previous close. A close too large for one frame
/// continues in frames of the same epoch and time, each but the last with `more` set.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `epoch` | epoch, 1 to 2⁴² − 1 |
/// | 8 | `time` | u64: microseconds of Unix time |
/// | 16 | `more` | u8 boolean: another frame of this close follows |
/// | 24 | `placement` | u64: the placement version taking effect at `epoch + 1`, 0 for none |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | catalog announcements, 16 bytes each ([`CatalogAnnouncement`]), by namespace then version |
/// | 1 | session evictions, 16 bytes each, ascending by their bytes |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpochClose {
    epoch: u64,
    time: u64,
    more: bool,
    placement: u64,
    announcements: Vec<CatalogAnnouncement>,
    evictions: Vec<[u8; 16]>,
}

impl EpochClose {
    /// The close of `epoch` at `time`, with no announcement or eviction yet, and no frame following.
    pub fn new(epoch: u64, time: u64) -> Self {
        Self {
            epoch,
            time,
            more: false,
            placement: 0,
            announcements: Vec::new(),
            evictions: Vec::new(),
        }
    }

    /// The close with another frame of it following.
    pub fn with_more(mut self) -> Self {
        self.more = true;
        self
    }

    /// The close announcing placement version `version`, 0 for none.
    pub fn with_placement(mut self, version: u64) -> Self {
        self.placement = version;
        self
    }

    /// `announcement` appended.
    pub fn push_announcement(&mut self, announcement: CatalogAnnouncement) {
        self.announcements.push(announcement);
    }

    /// The eviction of `session` appended.
    pub fn push_eviction(&mut self, session: [u8; 16]) {
        self.evictions.push(session);
    }

    /// The epoch closed.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The close's time, in microseconds of Unix time.
    pub fn time(&self) -> u64 {
        self.time
    }

    /// Whether another frame of this close follows.
    pub fn more(&self) -> bool {
        self.more
    }

    /// The placement version the close announces, 0 for none.
    pub fn placement(&self) -> u64 {
        self.placement
    }

    /// The catalog announcements.
    pub fn announcements(&self) -> &[CatalogAnnouncement] {
        &self.announcements
    }

    /// The evicted sessions.
    pub fn evictions(&self) -> &[[u8; 16]] {
        &self.evictions
    }
}
