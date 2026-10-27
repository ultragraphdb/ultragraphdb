/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod read_view_body_view;

use crate::types::quad_pattern::QuadPattern;

/// The body of a received Read frame, read in place; see [`crate::Read`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadView<'a> {
    namespace: u64,
    limit: u32,
    pattern: QuadPattern<&'a str>,
    resume: &'a [u8],
}

impl<'a> ReadView<'a> {
    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    pub fn limit(&self) -> u32 {
        self.limit
    }

    pub fn pattern(&self) -> &QuadPattern<&'a str> {
        &self.pattern
    }

    pub fn resume(&self) -> &'a [u8] {
        self.resume
    }
}
