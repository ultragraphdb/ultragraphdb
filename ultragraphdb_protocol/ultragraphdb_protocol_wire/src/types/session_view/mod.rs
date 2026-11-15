/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod session_view_body_view;

use std::marker::PhantomData;

/// The body of a received Session frame, read in place; see [`crate::Session`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionView<'a> {
    frame: PhantomData<&'a [u8]>,
    session: [u8; 16],
    principal: u64,
}

impl SessionView<'_> {
    /// The session's identity.
    #[must_use]
    pub fn session(&self) -> [u8; 16] {
        self.session
    }

    /// The principal the session belongs to.
    #[must_use]
    pub fn principal(&self) -> u64 {
        self.principal
    }
}
