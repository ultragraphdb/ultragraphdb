/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod epoch_close_view_body_view;

use crate::types::catalog_announcement::CatalogAnnouncement;
use crate::utils::whole_records::{record_at, record_count};

/// The body of a received EpochClose frame, read in place; see [`crate::EpochClose`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EpochCloseView<'a> {
    epoch: u64,
    time: u64,
    more: bool,
    placement: u64,
    announcements: &'a [u8],
    evictions: &'a [u8],
}

impl<'a> EpochCloseView<'a> {
    /// The epoch closed.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The close's time, in microseconds of Unix time.
    #[must_use]
    pub fn time(&self) -> u64 {
        self.time
    }

    /// Whether another frame of this close follows.
    #[must_use]
    pub fn more(&self) -> bool {
        self.more
    }

    /// The placement version the close announces, 0 for none.
    #[must_use]
    pub fn placement(&self) -> u64 {
        self.placement
    }

    /// The number of catalog announcements.
    #[must_use]
    pub fn announcement_count(&self) -> usize {
        record_count::<{ CatalogAnnouncement::SIZE }>(self.announcements)
    }

    /// Announcement `index`; `None` past the last.
    pub fn announcement(&self, index: usize) -> Option<CatalogAnnouncement> {
        record_at::<{ CatalogAnnouncement::SIZE }>(self.announcements, index)
            .map(CatalogAnnouncement::decode)
    }

    /// The number of evicted sessions.
    #[must_use]
    pub fn eviction_count(&self) -> usize {
        record_count::<16>(self.evictions)
    }

    /// Evicted session `index`; `None` past the last.
    #[must_use]
    pub fn eviction(&self, index: usize) -> Option<[u8; 16]> {
        record_at::<16>(self.evictions, index).copied()
    }
}
