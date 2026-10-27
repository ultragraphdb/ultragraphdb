/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod register_body_builder;

/// A session is registered (D9): sent with `session` zero, answered with the session's number.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `session` | u64 |
///
/// No sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Register {
    session: u64,
}

impl Register {
    pub fn new(session: u64) -> Self {
        Self { session }
    }

    pub fn session(&self) -> u64 {
        self.session
    }
}
