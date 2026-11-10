/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod catalog_view_body_view;

use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::le::bytes_at;

/// The body of a received Catalog frame, read in place; see [`crate::Catalog`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogView<'a> {
    tenant: u64,
    version: u64,
    epoch: u64,
    position: u64,
    bytes: &'a [u8],
}

// A copy moves at most one cache line (Ultra rule 9).
const _: () = assert!(size_of::<CatalogView<'static>>() <= 64);

impl<'a> CatalogView<'a> {
    /// The reply of continuation frames as one: the fields of `header`, a frame's command header its view checked, and
    /// `bytes`, every frame's section 0 concatenated.
    pub(crate) fn assembled(header: &[u8; COMMAND_HEADER_SIZE], bytes: &'a [u8]) -> Self {
        let field = |at: usize| bytes_at(header, at).map_or(0, u64::from_le_bytes);
        Self {
            tenant: field(0),
            version: field(8),
            epoch: field(16),
            position: field(24),
            bytes,
        }
    }

    /// The tenant.
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The catalog version; 0 in a request for the current one.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The epoch from which the version applies; 0 while pending, and in a request.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The metadata group's log position; 0 in a request.
    #[must_use]
    pub fn position(&self) -> u64 {
        self.position
    }

    /// Whether the body is a request: `position` 0.
    #[must_use]
    pub fn is_request(&self) -> bool {
        self.position == 0
    }

    /// The version's bytes; empty in a request.
    #[must_use]
    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }
}
