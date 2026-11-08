/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod challenge_view_body_view;

/// The body of a received Challenge frame, read in place; see [`crate::Challenge`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChallengeView<'a> {
    name: &'a str,
    iterations: u32,
    salt: &'a [u8],
}

// A copy moves at most one cache line (Ultra rule 9).
const _: () = assert!(size_of::<ChallengeView<'static>>() <= 64);

impl<'a> ChallengeView<'a> {
    /// The principal's name.
    #[must_use]
    pub fn name(&self) -> &'a str {
        self.name
    }

    /// The iteration count; 0 in a request.
    #[must_use]
    pub fn iterations(&self) -> u32 {
        self.iterations
    }

    /// The salt; empty in a request.
    #[must_use]
    pub fn salt(&self) -> &'a [u8] {
        self.salt
    }
}
