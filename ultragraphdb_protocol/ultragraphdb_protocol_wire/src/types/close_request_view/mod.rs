/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod close_request_view_body_view;

use std::marker::PhantomData;

/// The body of a received CloseRequest frame, read in place; see [`crate::CloseRequest`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloseRequestView<'a> {
    frame: PhantomData<&'a [u8]>,
    partition: u32,
    epoch: u64,
}

impl CloseRequestView<'_> {
    /// The asking partition; 0 for the frontend. Which sender may name which is a rule of the connection's role:
    /// only a frontend's connection may send 0, and only a storage process's a partition. The receiving session, which
    /// knows the role, enforces it (`PROTOCOL.md` §13.16); the view reads either.
    #[must_use]
    pub fn partition(&self) -> u32 {
        self.partition
    }

    /// The partition's open epoch.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
}
