/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod recall_view_body_view;

use std::marker::PhantomData;

/// The body of a received Recall frame, read in place; see [`crate::Recall`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecallView<'a> {
    frame: PhantomData<&'a [u8]>,
    partition: u32,
    session: [u8; 16],
    request: u64,
}

// A copy moves at most one cache line (Ultra rule 9).
const _: () = assert!(size_of::<RecallView<'static>>() <= 64);

impl RecallView<'_> {
    /// The partition asked.
    #[must_use]
    pub fn partition(&self) -> u32 {
        self.partition
    }

    /// The session.
    #[must_use]
    pub fn session(&self) -> [u8; 16] {
        self.session
    }

    /// The request number whose reply is recalled.
    #[must_use]
    pub fn request(&self) -> u64 {
        self.request
    }
}
