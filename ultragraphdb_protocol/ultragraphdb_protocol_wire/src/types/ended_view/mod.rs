/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod ended_view_body_view;

/// The size of one session's identity.
pub(crate) const SESSION_SIZE: usize = 16;

/// The body of a received Ended frame, read in place; see [`crate::Ended`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndedView<'a> {
    pub(crate) sessions: &'a [[u8; SESSION_SIZE]],
}

impl<'a> EndedView<'a> {
    /// The sessions, in order, borrowed from the frame; at least one.
    #[must_use]
    pub fn sessions(&self) -> &'a [[u8; SESSION_SIZE]] {
        self.sessions
    }
}
