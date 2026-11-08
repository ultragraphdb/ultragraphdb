/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod lease_view_body_view;

use std::marker::PhantomData;

/// The body of a received Lease frame, read in place; see [`crate::Lease`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaseView<'a> {
    frame: PhantomData<&'a [u8]>,
    type_prefix: u16,
    partition: u32,
    size: u64,
    start: u64,
    end: u64,
}

// A copy moves at most one cache line (Ultra rule 9).
const _: () = assert!(size_of::<LeaseView<'static>>() <= 64);

impl<'a> LeaseView<'a> {
    /// The type prefix.
    #[must_use]
    pub fn type_prefix(&self) -> u16 {
        self.type_prefix
    }

    /// The leasing partition.
    #[must_use]
    pub fn partition(&self) -> u32 {
        self.partition
    }

    /// The sequences asked for.
    #[must_use]
    pub fn size(&self) -> u64 {
        self.size
    }

    /// The block's first sequence; 0 in a request.
    #[must_use]
    pub fn start(&self) -> u64 {
        self.start
    }

    /// One past the block's last sequence; 0 in a request.
    #[must_use]
    pub fn end(&self) -> u64 {
        self.end
    }

    /// Whether the body is a request: `start` and `end` both 0.
    #[must_use]
    pub fn is_request(&self) -> bool {
        self.start == 0 && self.end == 0
    }
}
