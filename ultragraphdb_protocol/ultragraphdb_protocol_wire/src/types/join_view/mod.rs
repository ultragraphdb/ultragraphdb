/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod join_view_body_view;

use crate::types::role_kind::RoleKind;
use crate::utils::whole_records::{record_at, record_count};

/// The body of a received Join frame, read in place; see [`crate::Join`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JoinView<'a> {
    kind: RoleKind,
    address: &'a str,
    partitions: &'a [u8],
}

impl<'a> JoinView<'a> {
    /// The role's kind: compute or storage.
    #[must_use]
    pub fn kind(&self) -> RoleKind {
        self.kind
    }

    /// The role's role address.
    #[must_use]
    pub fn address(&self) -> &'a str {
        self.address
    }

    /// The number of partitions it hosts.
    #[must_use]
    pub fn partition_count(&self) -> usize {
        record_count::<4>(self.partitions)
    }

    /// Partition `index`; `None` past the last.
    #[must_use]
    pub fn partition(&self, index: usize) -> Option<u32> {
        record_at::<4>(self.partitions, index).map(|p| u32::from_le_bytes(*p))
    }
}
